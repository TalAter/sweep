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
fn word(raw: &str, start: usize) -> (usize, String) {
    let mut quote = None;
    let mut escaped = false;
    let mut decoded = String::new();
    for (offset, ch) in raw[start..].char_indices() {
        if escaped {
            decoded.push(ch);
            escaped = false;
        } else if ch == '\\' && quote != Some('\'') {
            escaped = true;
        } else if quote == Some(ch) {
            quote = None;
        } else if quote.is_none() && matches!(ch, '\'' | '"') {
            quote = Some(ch);
        } else if quote.is_none() && (ch.is_whitespace() || matches!(ch, '|' | ')')) {
            return (start + offset, decoded);
        } else {
            decoded.push(ch);
        }
    }
    (raw.len(), decoded)
}
fn value_end(raw: &str, start: usize) -> usize {
    word(raw, start).0
}

fn fetcher_secrets(raw: &str, spans: &mut Vec<(usize, usize)>) {
    // Also locate the fetcher inside the two supported substitution wrappers.
    let fetcher =
        regex::Regex::new(r"(?:^|[\s(])(?:[^\s()|]*/)?(?:curl|wget)\s+").expect("fetcher anchor");
    for found in fetcher.find_iter(raw) {
        let mut cursor = found.end();
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
    for flag in flags {
        let re = regex::Regex::new(&format!(r"(?:^|\s){}(=|\s|$)", regex::escape(flag)))
            .expect("escaped anchor");
        for cap in re.captures_iter(raw) {
            let mut start = cap.get(0).unwrap().end();
            if &cap[1] != "=" {
                start += raw[start..].len() - raw[start..].trim_start().len();
                if start == raw.len() || raw[start..].starts_with('-') {
                    continue;
                }
            }
            spans.push((start, value_end(raw, start)));
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
