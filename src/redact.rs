use crate::parse::InstallCommand;
use std::collections::BTreeSet;
fn secret(name: &str) -> bool {
    name.split(|c: char| !c.is_ascii_alphanumeric())
        .any(|part| {
            let upper = part.to_ascii_uppercase();
            [
                "KEY",
                "TOKEN",
                "SECRET",
                "PASSWORD",
                "PASSWD",
                "CREDENTIAL",
                "CREDENTIALS",
            ]
            .iter()
            .any(|suffix| upper.ends_with(suffix))
                || matches!(
                    upper.as_str(),
                    "KEY"
                        | "APIKEY"
                        | "TOKEN"
                        | "SECRET"
                        | "PASS"
                        | "PASSWORD"
                        | "PASSWD"
                        | "AUTH"
                        | "AUTHORIZATION"
                        | "CRED"
                        | "CREDENTIAL"
                        | "CREDENTIALS"
                )
        })
}

// Scan one literal shell word without evaluating expansions. Adjacent quoted and
// unquoted fragments belong to the same value, including escaped whitespace.
fn word(raw: &str, mut start: usize) -> (usize, String) {
    loop {
        let mut quote = None;
        let mut escaped = false;
        let mut end = raw.len();
        for (offset, ch) in raw[start..].char_indices() {
            if escaped {
                escaped = false;
            } else if ch == '\\' && quote != Some('\'') {
                escaped = true;
            } else if quote == Some(ch) {
                quote = None;
            } else if quote.is_none() && matches!(ch, '\'' | '"') {
                quote = Some(ch);
            } else if quote.is_none() && (ch.is_whitespace() || matches!(ch, '|' | ')')) {
                end = start + offset;
                break;
            }
        }
        let parsed = crate::parse::shell_word(&raw[start..end]);
        if parsed.as_ref().is_ok_and(|word| !word.present)
            && raw[end..].starts_with(char::is_whitespace)
        {
            start = end + raw[end..].len() - raw[end..].trim_start().len();
            continue;
        }
        let decoded = parsed
            .map(|word| word.text)
            .unwrap_or_else(|_| raw[start..end].to_owned());
        return (end, decoded);
    }
}
fn value_end(raw: &str, start: usize) -> usize {
    crate::parse::shell_word(&raw[start..])
        .map(|word| start + word.used)
        .unwrap_or(raw.len())
}

fn fetcher_secrets(raw: &str, spans: &mut Vec<(usize, usize)>) {
    // Decode executable words too: quoted names and removed line continuations
    // must have the same meaning here as they do in the install parser.
    let boundaries = regex::Regex::new(r"(?:^|[\s(])").expect("word boundary");
    let mut scanned_until = 0;
    for boundary in boundaries.find_iter(raw) {
        let start = boundary.end();
        if start < scanned_until && !raw[..start].ends_with('(') {
            continue;
        }
        let (mut cursor, executable) = word(raw, start);
        scanned_until = cursor;
        if !matches!(executable.rsplit('/').next(), Some("curl" | "wget")) {
            continue;
        }
        while cursor < raw.len() {
            cursor += raw[cursor..].len() - raw[cursor..].trim_start().len();
            let (end, token) = word(raw, cursor);
            if end == cursor {
                break;
            }
            let short_secret = token
                .strip_prefix('-')
                .filter(|s| !s.starts_with('-'))
                .and_then(|s| {
                    s.char_indices()
                        .find(|(_, ch)| matches!(ch, 'u' | 'U' | 'H'))
                });
            let (flag, attached) = if token.starts_with("--")
                && let Some((flag, _)) = token.split_once('=')
            {
                (flag, Some(flag.len() + 1))
            } else if let Some((index, ch)) = short_secret {
                let prefix = index + 2;
                (
                    match ch {
                        'u' => "-u",
                        'U' => "-U",
                        _ => "-H",
                    },
                    (prefix < token.len()).then_some(prefix),
                )
            } else {
                (token.as_str(), None)
            };
            let required_credential = matches!(
                flag,
                "-u" | "-U"
                    | "--user"
                    | "--proxy-user"
                    | "--password"
                    | "--http-user"
                    | "--http-password"
                    | "--ftp-user"
                    | "--ftp-password"
                    | "--proxy-password"
                    | "--oauth2-bearer"
                    | "--pass"
            );
            let credential = required_credential
                || (flag.starts_with("--") && flag != "--auth-no-challenge" && secret(flag));
            let header = matches!(flag, "-H" | "--header" | "--proxy-header");
            if !credential && !header {
                cursor = end;
                continue;
            }
            let (start, value_end, value) = if let Some(prefix) = attached {
                // Decoded offsets are valid only for a literal option prefix.
                // Otherwise hide the complete quoted/escaped option word.
                let start = if raw[cursor..].starts_with(&token[..prefix]) {
                    cursor + prefix
                } else {
                    cursor
                };
                (start, end, token[prefix..].to_owned())
            } else {
                let start = end + raw[end..].len() - raw[end..].trim_start().len();
                let (value_end, value) = word(raw, start);
                (start, value_end, value)
            };
            if attached.is_none()
                && (value_end == start
                    || (!required_credential && !header && value.starts_with('-')))
            {
                cursor = end;
                continue;
            }
            if credential
                || value
                    .split_once(':')
                    .is_some_and(|(name, _)| secret(name) || name.eq_ignore_ascii_case("cookie"))
            {
                spans.push((start, value_end));
            }
            cursor = value_end;
        }
    }
}
/// Redact anchored values, never every occurrence of matching secret bytes.
pub fn redact_command(cmd: &InstallCommand) -> String {
    let mut spans = Vec::new();
    let raw = &cmd.raw;
    fetcher_secrets(raw, &mut spans);
    for name in cmd.env_vars.keys().filter(|n| secret(n)) {
        let re = regex::Regex::new(&format!(r"(?:^|\s){}=", regex::escape(name)))
            .expect("escaped anchor");
        for m in re.find_iter(raw) {
            spans.push((m.end(), value_end(raw, m.end())));
        }
    }
    let flags: BTreeSet<_> = cmd
        .script_args
        .iter()
        .map(|s| s.split('=').next().unwrap_or(s))
        .filter(|s| s.starts_with('-') && secret(s))
        .collect();
    let mut tokens = Vec::new();
    let mut rest = raw.as_str();
    while !rest.is_empty() {
        rest = rest.trim_start();
        if rest.is_empty() {
            break;
        }
        let start = raw.len() - rest.len();
        let (word, used, present) = crate::parse::shell_word(rest)
            .map(|word| (word.text, word.used, word.present))
            .unwrap_or_else(|_| (rest.to_owned(), rest.len(), true));
        if present {
            tokens.push((start, start + used, word));
        }
        rest = &rest[used..];
    }
    for (i, (start, end, word)) in tokens.iter().enumerate() {
        let (flag, inline) = word
            .split_once('=')
            .map_or((word.as_str(), false), |(f, _)| (f, true));
        if !flags.contains(flag) {
            continue;
        }
        if inline {
            // Preserve conventional flag spelling; quoted/concatenated spellings
            // need the whole source word redacted to avoid leaving secret bytes.
            let prefix = format!("{flag}=");
            let value_start = if raw[*start..*end].starts_with(&prefix) {
                start + prefix.len()
            } else {
                *start
            };
            spans.push((value_start, *end));
        } else if let Some((value_start, value_end, _)) = tokens.get(i + 1)
            && !raw[*value_start..*value_end].starts_with('-')
        {
            spans.push((*value_start, *value_end));
        }
    }
    spans.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (start, end) in spans {
        if let Some(last) = merged.last_mut()
            && start < last.1
        {
            last.1 = last.1.max(end);
            continue;
        }
        merged.push((start, end));
    }
    let mut out = String::with_capacity(raw.len());
    let mut cursor = 0;
    for (start, end) in merged {
        out.push_str(&raw[cursor..start]);
        out.push_str("<redacted>");
        cursor = end;
    }
    out.push_str(&raw[cursor..]);
    out
}
