//! Static table of LLM providers the setup wizard can configure.
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Api,
    Cli { binary: &'static str },
}

#[derive(Debug, PartialEq, Eq)]
pub struct Provider {
    pub key: &'static str,
    pub display: &'static str,
    pub kind: Kind,
    pub key_url: Option<&'static str>,
    pub placeholder: Option<&'static str>,
    pub base_url: Option<&'static str>,
    pub recommended: Option<&'static str>,
    pub glyph: char,
}

const fn api(
    key: &'static str,
    display: &'static str,
    key_url: Option<&'static str>,
    placeholder: Option<&'static str>,
    base_url: Option<&'static str>,
    recommended: Option<&'static str>,
    glyph: char,
) -> Provider {
    Provider {
        key,
        display,
        kind: Kind::Api,
        key_url,
        placeholder,
        base_url,
        recommended,
        glyph,
    }
}

/// Spec table order; also the order providers are written to `config.jsonc`.
pub const PROVIDERS: &[Provider] = &[
    api(
        "anthropic",
        "Anthropic",
        Some("https://console.anthropic.com/settings/keys"),
        Some("sk-ant-api03-"),
        None,
        Some(r"^claude-sonnet(-\d+)+$"),
        '\u{E754}',
    ),
    api(
        "openai",
        "OpenAI",
        Some("https://platform.openai.com/api-keys"),
        Some("sk-proj-"),
        None,
        Some(r"^gpt-5(\.\d+)?$"),
        '\u{F0104}',
    ),
    api(
        "openrouter",
        "OpenRouter",
        Some("https://openrouter.ai/keys"),
        Some("sk-or-v1-"),
        Some("https://openrouter.ai/api/v1"),
        None,
        '\u{EA63}',
    ),
    api(
        "groq",
        "Groq",
        Some("https://console.groq.com/keys"),
        Some("gsk_"),
        Some("https://api.groq.com/openai/v1"),
        None,
        '\u{F140B}',
    ),
    api(
        "mistral",
        "Mistral",
        Some("https://console.mistral.ai/api-keys"),
        None,
        Some("https://api.mistral.ai/v1"),
        None,
        '\u{EF16}',
    ),
    api(
        "ollama",
        "Ollama (local)",
        None,
        None,
        Some("http://localhost:11434/v1"),
        None,
        '\u{F019A}',
    ),
    Provider {
        key: "claude-code",
        display: "Claude Code",
        kind: Kind::Cli { binary: "claude" },
        key_url: None,
        placeholder: None,
        base_url: None,
        recommended: None,
        glyph: '\u{F0BC9}',
    },
];

pub fn find(key: &str) -> Option<&'static Provider> {
    PROVIDERS.iter().find(|p| p.key == key)
}

/// API providers always; CLI providers only when their binary is on `PATH` from `env`.
pub fn available(env: &HashMap<String, String>) -> Vec<&'static Provider> {
    PROVIDERS
        .iter()
        .filter(|p| match p.kind {
            Kind::Api => true,
            Kind::Cli { binary } => on_path(binary, env),
        })
        .collect()
}

fn on_path(binary: &str, env: &HashMap<String, String>) -> bool {
    env.get("PATH")
        .map(|path| std::env::split_paths(path).any(|dir| is_executable(&dir.join(binary))))
        .unwrap_or(false)
}

fn is_executable(path: &std::path::Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.is_file() && meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        meta.is_file()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_resolves_known_keys_with_endpoints_and_rejects_unknown() {
        assert!(find("nope").is_none());
        for key in ["groq", "mistral", "ollama"] {
            assert!(find(key).unwrap().base_url.is_some(), "{key}");
        }
        assert_eq!(
            find("claude-code").unwrap().kind,
            Kind::Cli { binary: "claude" }
        );
    }

    #[cfg(unix)]
    fn write_executable(dir: &std::path::Path, name: &str) {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join(name);
        std::fs::write(&path, "#!/bin/sh\ntrue\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn cli_provider_hidden_unless_binary_is_executable_on_path() {
        let with = tempfile::tempdir().unwrap();
        let without = tempfile::tempdir().unwrap();
        write_executable(with.path(), "claude");
        std::fs::write(without.path().join("claude"), "not executable").unwrap();
        let path = |dir: &tempfile::TempDir| {
            HashMap::from([("PATH".to_string(), dir.path().display().to_string())])
        };
        let keys = |env| available(&env).iter().map(|p| p.key).collect::<Vec<_>>();
        assert_eq!(
            keys(path(&with)),
            [
                "anthropic",
                "openai",
                "openrouter",
                "groq",
                "mistral",
                "ollama",
                "claude-code"
            ]
        );
        assert_eq!(
            keys(path(&without)),
            [
                "anthropic",
                "openai",
                "openrouter",
                "groq",
                "mistral",
                "ollama"
            ]
        );
        assert!(!keys(HashMap::new()).contains(&"claude-code"));
    }
}
