use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallCommand {
    pub env_vars: BTreeMap<String, String>,
    pub sudo: bool,
    pub shell: String,
    pub script_args: Vec<String>,
    pub url: String,
    pub raw: String,
}
#[derive(Debug, Clone, thiserror::Error)]
#[error("{message}")]
pub struct ParseError {
    pub kind: String,
    pub message: String,
}
fn error(kind: &str, message: impl Into<String>) -> ParseError {
    ParseError {
        kind: kind.into(),
        message: message.into(),
    }
}
fn bare(s: &str) -> &str {
    s.rsplit('/').next().unwrap_or(s)
}
fn fetcher(s: &str) -> bool {
    matches!(bare(s), "curl" | "wget")
}
fn shell(s: &str) -> bool {
    matches!(bare(s), "sh" | "bash" | "zsh")
}
fn skip_sudo(tokens: &[&str]) -> usize {
    if tokens.first() != Some(&"sudo") {
        return 0;
    }
    let mut i = 1;
    while tokens.get(i).is_some_and(|s| s.starts_with('-')) {
        i += 1;
    }
    i
}
fn last_url(s: &str) -> Result<String, ParseError> {
    regex::Regex::new(r"https?://\S+")
        .unwrap()
        .find_iter(s)
        .last()
        .map(|m| {
            m.as_str()
                .trim_end_matches([')', '\'', '"', '`'])
                .to_owned()
        })
        .ok_or_else(|| error("no-url", "no URL found in install command"))
}
/// Recognize only the finite installer grammar; this never evaluates shell input.
pub fn parse_install_command(input: &str) -> Result<InstallCommand, ParseError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(error("empty", "empty input"));
    }
    let mut quote = None;
    let mut pipes = Vec::new();
    let bytes = trimmed.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if let Some(q) = quote {
            if b == q {
                quote = None
            }
            continue;
        }
        if b == b'\'' || b == b'"' {
            quote = Some(b);
            continue;
        }
        if b == b';' || ((b == b'&' || b == b'|') && bytes.get(i + 1) == Some(&b)) {
            return Err(error(
                "chain",
                "chained commands (&&, ||, ;) are refused — paste a single install command",
            ));
        }
        if b == b'|' {
            pipes.push(i)
        }
    }
    let env_re = regex::Regex::new(r"^([A-Z_][A-Z0-9_]*)=").unwrap();
    let mut env_vars = BTreeMap::new();
    let mut rest = trimmed;
    while let Some(cap) = env_re.captures(rest) {
        let name = cap[1].to_owned();
        let value = &rest[cap[0].len()..];
        let (value_text, used) = if value.starts_with(['\'', '"']) {
            let end = value[1..].find(value.as_bytes()[0] as char).map(|i| i + 1);
            match end {
                Some(end) => (value[1..end].to_owned(), end + 1),
                None => (value[1..].to_owned(), value.len()),
            }
        } else {
            let end = value.find(char::is_whitespace).unwrap_or(value.len());
            (value[..end].to_owned(), end)
        };
        env_vars.insert(name, value_text);
        rest = value[used..].trim_start();
    }
    let mut cmd = InstallCommand {
        env_vars,
        sudo: false,
        shell: String::new(),
        script_args: vec![],
        url: String::new(),
        raw: input.into(),
    };
    let process = rest.contains("<(");
    let substitution = regex::Regex::new(r#"-c\s+["']?\$\("#)
        .unwrap()
        .is_match(rest);
    if process || substitution {
        let pattern = if process {
            r"^(?:(sudo(?:\s+-\S+)*)\s+)?(?:\S*/)?(sh|bash|zsh)\s+<\(([^)]*)\)\s*$"
        } else {
            r#"^(?:(sudo(?:\s+-\S+)*)\s+)?(?:\S*/)?(sh|bash|zsh)\s+-c\s+["']?\$\(([^)]*)\)["']?\s*$"#
        };
        let label = if process { "process-subst" } else { "$()" };
        let cap = regex::Regex::new(pattern)
            .unwrap()
            .captures(rest)
            .ok_or_else(|| {
                error(
                    "unsupported",
                    format!("cannot recognize {label} shape: {rest}"),
                )
            })?;
        let inner = &cap[3];
        if !inner.split_whitespace().next().is_some_and(fetcher) {
            return Err(error(
                "unsupported",
                format!("expected curl or wget inside substitution: {inner}"),
            ));
        }
        cmd.url = last_url(inner)?;
        cmd.sudo = cap.get(1).is_some();
        cmd.shell = cap[2].into();
        return Ok(cmd);
    }
    // Scan offsets above refer to trimmed input, before consuming assignments.
    let offset = trimmed.len() - rest.len();
    let pipes: Vec<_> = pipes
        .into_iter()
        .filter_map(|p| p.checked_sub(offset))
        .collect();
    if pipes.is_empty() {
        let tokens: Vec<_> = rest.split_whitespace().collect();
        return Err(
            if tokens.get(skip_sudo(&tokens)).is_some_and(|s| fetcher(s)) {
                error(
                    "no-pipe",
                    "expected `<fetcher> <url> | <shell>` but found no pipe",
                )
            } else {
                error(
                    "unsupported",
                    format!("unrecognized install command shape: {rest}"),
                )
            },
        );
    }
    if pipes.len() > 1 {
        return Err(error("unsupported", "multiple pipes are not supported"));
    }
    let lhs = rest[..pipes[0]].trim();
    let rhs = rest[pipes[0] + 1..].trim();
    let left: Vec<_> = lhs.split_whitespace().collect();
    if !left.get(skip_sudo(&left)).is_some_and(|s| fetcher(s)) {
        return Err(error(
            "no-fetcher",
            format!("left side of pipe must start with curl or wget: {lhs}"),
        ));
    }
    cmd.url = last_url(lhs)?;
    let right: Vec<_> = rhs.split_whitespace().collect();
    let mut i = skip_sudo(&right);
    if !right.get(i).is_some_and(|s| shell(s)) {
        return Err(error(
            "unsupported",
            format!("right side of pipe must be sh, bash, or zsh: {rhs}"),
        ));
    }
    cmd.sudo = right.first() == Some(&"sudo");
    cmd.shell = bare(right[i]).into();
    i += 1;
    if right.get(i) == Some(&"-s") {
        i += 1
    }
    if right.get(i) == Some(&"--") {
        i += 1
    }
    cmd.script_args = right[i..].iter().map(|s| s.to_string()).collect();
    Ok(cmd)
}
pub fn slug_from_url(url: &str) -> String {
    let Ok(url) = url::Url::parse(url) else {
        return "unknown".into();
    };
    let Some(host) = url.host_str() else {
        return "unknown".into();
    };
    let host = host.to_lowercase();
    let mut host = host.as_str();
    for prefix in ["www.", "get.", "install.", "download.", "dl.", "cdn."] {
        if let Some(stripped) = host.strip_prefix(prefix) {
            host = stripped;
            break;
        }
    }
    host.split('.')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or("unknown")
        .into()
}
