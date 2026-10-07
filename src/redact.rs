use crate::parse::InstallCommand;
use std::collections::BTreeSet;
fn secret(name: &str) -> bool {
    let upper = name.to_uppercase();
    ["KEY", "TOKEN", "SECRET", "PASS", "AUTH", "CRED"]
        .iter()
        .any(|k| upper.contains(k))
}
fn value_end(raw: &str, start: usize) -> usize {
    crate::parse::shell_word(&raw[start..])
        .map(|word| start + word.used)
        .unwrap_or(raw.len())
}
/// Redact anchored values, never every occurrence of matching secret bytes.
pub fn redact_command(cmd: &InstallCommand) -> String {
    let mut spans = Vec::new();
    let raw = &cmd.raw;
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
