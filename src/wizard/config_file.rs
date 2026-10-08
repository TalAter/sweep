//! Renders and writes `config.jsonc` plus its JSON schema from wizard answers.
use super::providers::{self, PROVIDERS};
use crate::{atomic_file, config::resolve_provider};
use serde_json::{Value, json};
use std::{collections::HashMap, path::Path};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderEntry {
    pub key: String,
    /// `None` for CLI providers.
    pub api_key: Option<String>,
    /// `None` for CLI providers.
    pub model: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WizardConfig {
    pub providers: Vec<ProviderEntry>,
    pub default_provider: Option<String>,
    pub nerd_fonts: bool,
}

/// Entries in provider-table order; entries for unknown keys follow in input order.
fn ordered(config: &WizardConfig) -> Vec<&ProviderEntry> {
    let known = PROVIDERS
        .iter()
        .filter_map(|p| config.providers.iter().find(|e| e.key == p.key));
    let unknown = config
        .providers
        .iter()
        .filter(|e| providers::find(&e.key).is_none());
    known.chain(unknown).collect()
}

/// `(json key, value)` pairs in written order: `baseURL`, `apiKey`, `model`.
fn entry_fields(entry: &ProviderEntry) -> Vec<(&'static str, String)> {
    let base_url = providers::find(&entry.key).and_then(|p| p.base_url);
    [
        ("baseURL", base_url.map(str::to_string)),
        (
            "apiKey",
            entry.api_key.as_deref().map(|k| k.trim().to_string()),
        ),
        ("model", entry.model.clone()),
    ]
    .into_iter()
    .filter_map(|(k, v)| v.map(|v| (k, v)))
    .collect()
}

fn entry_json(entry: &ProviderEntry) -> Value {
    Value::Object(
        entry_fields(entry)
            .into_iter()
            .map(|(k, v)| (k.to_string(), Value::String(v)))
            .collect(),
    )
}

fn quote(s: &str) -> String {
    serde_json::to_string(s).expect("string serializes")
}

/// Plain JSON, 2-space indent, one line per provider entry.
pub fn render_config(config: &WizardConfig) -> String {
    let mut out = String::from("{\n  \"$schema\": \"./config.schema.json\",\n");
    let entries = ordered(config);
    if entries.is_empty() {
        out.push_str("  \"providers\": {},\n");
    } else {
        out.push_str("  \"providers\": {\n");
        let lines: Vec<String> = entries
            .iter()
            .map(|entry| {
                let fields: Vec<String> = entry_fields(entry)
                    .into_iter()
                    .map(|(k, v)| format!("{}: {}", quote(k), quote(&v)))
                    .collect();
                let body = if fields.is_empty() {
                    "{}".to_string()
                } else {
                    format!("{{ {} }}", fields.join(", "))
                };
                format!("    {}: {body}", quote(&entry.key))
            })
            .collect();
        out.push_str(&lines.join(",\n"));
        out.push_str("\n  },\n");
    }
    if let Some(default) = &config.default_provider {
        out.push_str(&format!("  \"defaultProvider\": {},\n", quote(default)));
    }
    out.push_str(&format!("  \"nerdFonts\": {}\n}}\n", config.nerd_fonts));
    out
}

pub fn render_schema() -> String {
    let schema = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "title": "Sweep configuration file",
        "type": "object",
        "additionalProperties": true,
        "properties": {
            "$schema": {
                "type": "string",
                "description": "Path or URL of the JSON schema describing this file."
            },
            "providers": {
                "type": "object",
                "description": "LLM providers keyed by name (anthropic, openai, openrouter, groq, mistral, ollama, claude-code, or a custom OpenAI-compatible endpoint).",
                "additionalProperties": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "apiKey": {
                            "type": "string",
                            "description": "API key, or \"$ENV_VAR\" to read it from an environment variable at startup."
                        },
                        "baseURL": {
                            "type": "string",
                            "description": "API endpoint base URL; required for groq, mistral, ollama, and custom providers."
                        },
                        "model": {
                            "type": "string",
                            "description": "Model identifier to request from the provider."
                        }
                    }
                }
            },
            "defaultProvider": {
                "type": "string",
                "description": "Name of the provider in \"providers\" used for analysis."
            },
            "nerdFonts": {
                "type": "boolean",
                "default": false,
                "description": "Render Nerd Font glyphs in the terminal UI."
            }
        }
    });
    let mut text = serde_json::to_string_pretty(&schema).expect("schema serializes");
    text.push('\n');
    text
}

/// Rejects anything `resolve_provider` would reject when that entry is the default.
pub fn validate(config: &WizardConfig, env: &HashMap<String, String>) -> Result<(), String> {
    for entry in &config.providers {
        let seen = json!({
            "defaultProvider": entry.key,
            "providers": { entry.key.clone(): entry_json(entry) },
        });
        resolve_provider(&seen, env)?;
    }
    if let Some(default) = &config.default_provider
        && !config.providers.iter().any(|e| &e.key == default)
    {
        return Err(format!("provider \"{default}\" not found in config."));
    }
    Ok(())
}

/// Validates, then writes `config.jsonc` and `config.schema.json` under `home`.
/// Both files are fully prepared before either gets its name, and the config is
/// never written over a file that appeared while the wizard ran. A failure
/// publishes nothing and leaves no temp files. The config holds API keys, so it
/// is owner-readable only; the schema is world-readable.
pub fn write(
    home: &Path,
    config: &WizardConfig,
    env: &HashMap<String, String>,
) -> Result<(), String> {
    validate(config, env)?;
    let config_path = home.join("config.jsonc");
    let schema_path = home.join("config.schema.json");
    let prepared_config =
        atomic_file::prepare(&config_path, render_config(config).as_bytes(), 0o600)
            .map_err(|e| format!("cannot write config.jsonc: {e}"))?;
    let prepared_schema = atomic_file::prepare(&schema_path, render_schema().as_bytes(), 0o644)
        .map_err(|e| format!("cannot write config.schema.json: {e}"))?;
    prepared_config.create_new().map_err(|e| {
        if e.kind() == std::io::ErrorKind::AlreadyExists {
            format!(
                "{} was created while setup was running; leaving it untouched.",
                config_path.display()
            )
        } else {
            format!("cannot write config.jsonc: {e}")
        }
    })?;
    if let Err(e) = prepared_schema.replace() {
        let _ = std::fs::remove_file(&config_path);
        return Err(format!("cannot write config.schema.json: {e}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config;
    use serde_json::Value;

    fn entry(key: &str, api_key: Option<&str>, model: Option<&str>) -> ProviderEntry {
        ProviderEntry {
            key: key.into(),
            api_key: api_key.map(str::to_string),
            model: model.map(str::to_string),
        }
    }

    fn spec_example() -> WizardConfig {
        WizardConfig {
            providers: vec![
                entry("claude-code", None, None),
                entry("openai", Some(" sk-proj-XXXX "), Some("gpt-5")),
                entry(
                    "anthropic",
                    Some("sk-ant-api03-XXXX"),
                    Some("claude-sonnet-4-6"),
                ),
            ],
            default_provider: Some("openai".into()),
            nerd_fonts: true,
        }
    }

    #[test]
    fn renders_spec_example_in_table_order_with_trimmed_keys() {
        let expected = r#"{
  "$schema": "./config.schema.json",
  "providers": {
    "anthropic": { "apiKey": "sk-ant-api03-XXXX", "model": "claude-sonnet-4-6" },
    "openai": { "apiKey": "sk-proj-XXXX", "model": "gpt-5" },
    "claude-code": {}
  },
  "defaultProvider": "openai",
  "nerdFonts": true
}
"#;
        assert_eq!(render_config(&spec_example()), expected);
    }

    #[test]
    fn renders_empty_providers_without_default() {
        let expected = r#"{
  "$schema": "./config.schema.json",
  "providers": {},
  "nerdFonts": false
}
"#;
        assert_eq!(render_config(&WizardConfig::default()), expected);
    }

    #[test]
    fn compatibility_providers_get_base_url_and_escaped_strings() {
        let config = WizardConfig {
            providers: vec![entry("groq", Some("gsk_\"q\""), Some("m"))],
            default_provider: None,
            nerd_fonts: false,
        };
        let rendered = render_config(&config);
        assert!(rendered.contains(
            r#""groq": { "baseURL": "https://api.groq.com/openai/v1", "apiKey": "gsk_\"q\"", "model": "m" }"#
        ));
        let parsed: Value = serde_json::from_str(&rendered).unwrap();
        assert_eq!(parsed["providers"]["groq"]["apiKey"], "gsk_\"q\"");
    }

    #[test]
    fn written_config_round_trips_through_loader_for_every_provider() {
        let home = tempfile::tempdir().unwrap();
        let env = HashMap::new();
        let mut config = spec_example();
        for key in ["groq", "mistral", "openrouter"] {
            config.providers.push(entry(key, Some("k"), Some("m")));
        }
        config.providers.push(entry("ollama", None, Some("llama3")));
        write(home.path(), &config, &env).unwrap();
        let loaded = config::load(home.path(), &env).unwrap();
        assert_eq!(loaded["nerdFonts"], true);
        assert_eq!(loaded["defaultProvider"], "openai");
        for entry in &config.providers {
            let mut c = loaded.clone();
            c["defaultProvider"] = Value::String(entry.key.clone());
            let resolved =
                config::resolve_provider(&c, &env).unwrap_or_else(|e| panic!("{}: {e}", entry.key));
            assert_eq!(resolved.model, entry.model);
        }
        let schema: Value = serde_json::from_str(
            &std::fs::read_to_string(home.path().join("config.schema.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(schema["title"], "Sweep configuration file");
    }

    #[test]
    fn rejects_entries_the_session_would_reject_and_writes_nothing() {
        let home = tempfile::tempdir().unwrap();
        let env = HashMap::new();
        let config = WizardConfig {
            providers: vec![entry("anthropic", Some("k"), None)],
            default_provider: Some("anthropic".into()),
            nerd_fonts: false,
        };
        let expected = "provider \"anthropic\" has no model set in config.";
        assert_eq!(validate(&config, &env).unwrap_err(), expected);
        assert_eq!(write(home.path(), &config, &env).unwrap_err(), expected);
        assert!(!home.path().join("config.jsonc").exists());
        assert!(!home.path().join("config.schema.json").exists());
    }

    #[test]
    fn failed_write_leaves_no_partial_or_temporary_files() {
        let env = HashMap::new();
        let config = spec_example();
        let names = |home: &Path| {
            let mut names: Vec<String> = std::fs::read_dir(home)
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            names.sort();
            names
        };

        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(home.path().join("config.schema.json/blocker")).unwrap();
        assert!(write(home.path(), &config, &env).is_err());
        assert_eq!(
            names(home.path()),
            ["config.schema.json"],
            "schema fails first"
        );

        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(home.path().join("config.jsonc/blocker")).unwrap();
        assert!(write(home.path(), &config, &env).is_err());
        assert_eq!(
            names(home.path()),
            ["config.jsonc"],
            "no orphan schema when the config cannot be published"
        );

        let home = tempfile::tempdir().unwrap();
        write(home.path(), &config, &env).unwrap();
        assert_eq!(names(home.path()), ["config.jsonc", "config.schema.json"]);
    }

    #[test]
    fn config_that_appeared_meanwhile_is_left_untouched() {
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join("config.jsonc");
        std::fs::write(&path, "{ \"theirs\": true }").unwrap();
        let error = write(home.path(), &spec_example(), &HashMap::new()).unwrap_err();
        assert_eq!(
            error,
            format!(
                "{} was created while setup was running; leaving it untouched.",
                path.display()
            )
        );
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "{ \"theirs\": true }"
        );
        let mut names: Vec<String> = std::fs::read_dir(home.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, ["config.jsonc"], "no schema, no temp files");
    }

    #[cfg(unix)]
    #[test]
    fn config_is_owner_readable_only_and_schema_is_world_readable() {
        use std::os::unix::fs::PermissionsExt;
        let home = tempfile::tempdir().unwrap();
        write(home.path(), &spec_example(), &HashMap::new()).unwrap();
        let mode = |name: &str| {
            std::fs::metadata(home.path().join(name))
                .unwrap()
                .permissions()
                .mode()
                & 0o777
        };
        assert_eq!(mode("config.jsonc"), 0o600);
        assert_eq!(mode("config.schema.json"), 0o644);
    }

    #[test]
    fn rejects_default_not_among_providers_but_accepts_empty_config() {
        let env = HashMap::new();
        let config = WizardConfig {
            providers: vec![entry("claude-code", None, None)],
            default_provider: Some("openai".into()),
            nerd_fonts: false,
        };
        assert!(validate(&config, &env).is_err());
        assert_eq!(validate(&WizardConfig::default(), &env), Ok(()));
    }

    #[test]
    fn schema_describes_fields_and_locks_provider_entries() {
        let schema: Value = serde_json::from_str(&render_schema()).unwrap();
        assert_eq!(schema["$schema"], "http://json-schema.org/draft-07/schema#");
        assert_eq!(schema["title"], "Sweep configuration file");
        assert_eq!(schema["additionalProperties"], true);
        let entry = &schema["properties"]["providers"]["additionalProperties"];
        assert_eq!(entry["additionalProperties"], false);
        assert!(
            entry["properties"]["apiKey"]["description"]
                .as_str()
                .unwrap()
                .contains("\"$ENV_VAR\"")
        );
    }
}
