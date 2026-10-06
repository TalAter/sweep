use crate::parse::InstallCommand;
use std::collections::BTreeSet;
fn secret(name: &str) -> bool {
    let upper = name.to_uppercase();
    ["KEY", "TOKEN", "SECRET", "PASS", "AUTH", "CRED"]
        .iter()
        .any(|k| upper.contains(k))
}
fn value_end(raw: &str, start: usize) -> usize {
    let value = &raw[start..];
    if value.starts_with(['\'', '"']) {
        value[1..]
            .find(value.as_bytes()[0] as char)
            .map(|n| start + n + 2)
            .unwrap_or(raw.len())
    } else {
        start + value.find(char::is_whitespace).unwrap_or(value.len())
    }
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
