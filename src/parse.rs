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
// Return decoded text and its source length so assignments and redaction share
// the same word boundaries. No shell is invoked.
pub(crate) struct ShellWord {
    pub text: String,
    pub used: usize,
    expansion: bool,
    pub present: bool,
}
pub(crate) fn shell_word(s: &str) -> Result<ShellWord, ParseError> {
    let mut word = String::new();
    let mut expansion = false;
    let mut present = false;
    let mut quote = None;
    let mut chars = s.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if quote.is_none() && c.is_whitespace() {
            return Ok(ShellWord {
                text: word,
                used: i,
                expansion,
                present,
            });
        }
        let at_start = !present;
        // A removed line continuation does not itself form an empty argument.
        if !(c == '\\'
            && quote != Some('\'')
            && chars.peek().is_some_and(|(_, next)| *next == '\n'))
        {
            present = true;
        }
        match c {
            '\'' | '"' if quote.is_none() => quote = Some(c),
            c if quote == Some(c) => quote = None,
            '\\' if quote != Some('\'') => {
                let Some(&(_, next)) = chars.peek() else {
                    return Err(error("unsupported", "unfinished escape in install command"));
                };
                if quote == Some('"') && !matches!(next, '$' | '`' | '"' | '\\' | '\n') {
                    word.push(c);
                } else {
                    chars.next();
                    if next != '\n' {
                        word.push(next);
                    }
                }
            }
            _ => {
                if quote != Some('\'')
                    && (c == '`' || c == '$' || (c == '~' && quote.is_none() && at_start))
                {
                    expansion = true;
                }
                word.push(c);
            }
        }
    }
    if quote.is_some() {
        return Err(error("unsupported", "unclosed quote in install command"));
    }
    Ok(ShellWord {
        text: word,
        used: s.len(),
        expansion,
        present,
    })
}
fn literal_word(s: &str) -> Result<ShellWord, ParseError> {
    let word = shell_word(s)?;
    if word.expansion {
        return Err(error(
            "unsupported",
            "shell expansion is not supported; replace variables and ~ with explicit values, or single-quote literal text",
        ));
    }
    Ok(word)
}
fn words(mut s: &str) -> Result<Vec<String>, ParseError> {
    let mut tokens = Vec::new();
    s = s.trim_start();
    while !s.is_empty() {
        let word = literal_word(s)?;
        if word.present {
            tokens.push(word.text);
        }
        s = s[word.used..].trim_start();
    }
    Ok(tokens)
}
fn skip_sudo(tokens: &[String]) -> Result<usize, ParseError> {
    if !tokens.first().is_some_and(|s| bare(s) == "sudo") {
        return Ok(0);
    }
    if tokens.get(1).is_some_and(|s| s.starts_with('-')) {
        return Err(error(
            "unsupported",
            "sudo options are not supported; use plain sudo or remove it",
        ));
    }
    Ok(1)
}
fn single_url(tokens: &[String]) -> Result<String, ParseError> {
    let mut urls = tokens
        .iter()
        .map(|s| s.strip_prefix("--url=").unwrap_or(s))
        .filter(|s| s.starts_with("https://") || s.starts_with("http://"));
    let url = urls
        .next()
        .ok_or_else(|| error("no-url", "no URL found in install command"))?;
    if urls.next().is_some() {
        return Err(error(
            "unsupported",
            "multiple URLs are not supported; paste a single installer URL",
        ));
    }
    Ok(url.to_owned())
}
/// Recognize only the finite installer grammar; this never evaluates shell input.
pub fn parse_install_command(input: &str) -> Result<InstallCommand, ParseError> {
    let trimmed = input.trim_start();
    if trimmed.is_empty() {
        return Err(error("empty", "empty input"));
    }
    let mut quote = None;
    let mut escaped = false;
    let mut pipes = Vec::new();
    let bytes = trimmed.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if escaped {
            escaped = false;
            continue;
        }
        if b == b'\\' && quote != Some(b'\'') {
            escaped = true;
            continue;
        }
        if let Some(q) = quote {
            if b == q {
                quote = None;
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
            pipes.push(i);
        }
    }
    if quote.is_some() || escaped {
        return Err(error(
            "unsupported",
            "unclosed quote or unfinished escape in install command",
        ));
    }
    let env_re = regex::Regex::new(r"^([A-Z_][A-Z0-9_]*)=").unwrap();
    let mut env_vars = BTreeMap::new();
    let mut rest = trimmed;
    while let Some(cap) = env_re.captures(rest) {
        let name = cap[1].to_owned();
        let value = &rest[cap[0].len()..];
        let word = literal_word(value)?;
        env_vars.insert(name, word.text);
        rest = value[word.used..].trim_start();
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
    if pipes.is_empty() && (process || substitution) {
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
        let inner_tokens = words(inner)?;
        if !inner_tokens.first().is_some_and(|s| fetcher(s)) {
            return Err(error(
                "unsupported",
                format!("expected curl or wget inside substitution: {inner}"),
            ));
        }
        if let Some(sudo) = cap.get(1) {
            skip_sudo(&words(sudo.as_str())?)?;
        }
        cmd.url = single_url(&inner_tokens)?;
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
        let tokens = words(rest)?;
        return Err(
            if tokens.get(skip_sudo(&tokens)?).is_some_and(|s| fetcher(s)) {
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
    let lhs = &rest[..pipes[0]];
    let rhs = &rest[pipes[0] + 1..];
    let left = words(lhs)?;
    if !left.get(skip_sudo(&left)?).is_some_and(|s| fetcher(s)) {
        return Err(error(
            "no-fetcher",
            format!("left side of pipe must start with curl or wget: {lhs}"),
        ));
    }
    cmd.url = single_url(&left)?;
    let right = words(rhs)?;
    let mut i = skip_sudo(&right)?;
    if !right.get(i).is_some_and(|s| shell(s)) {
        return Err(error(
            "unsupported",
            format!("right side of pipe must be sh, bash, or zsh: {rhs}"),
        ));
    }
    cmd.sudo = i == 1;
    cmd.shell = bare(&right[i]).into();
    i += 1;
    if right.get(i).is_some_and(|s| s == "-s") {
        i += 1
    }
    if right.get(i).is_some_and(|s| s == "--") {
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
