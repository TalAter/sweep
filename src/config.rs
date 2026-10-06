use serde_json::Value;
use std::{collections::HashMap, path::Path};
#[derive(Clone, Debug)]
pub struct ResolvedProvider {
    pub name: String,
    pub model: Option<String>,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
}
/// JSONC on disk; the environment override is strict JSON and shallow-merges.
pub fn load(home: &Path, env: &HashMap<String, String>) -> Result<Value, String> {
    let mut config = match std::fs::read_to_string(home.join("config.jsonc")) {
        Ok(raw) => jsonc_parser::parse_to_serde_value::<Option<Value>>(
            &raw,
            &jsonc_parser::ParseOptions {
                allow_comments: true,
                allow_trailing_commas: true,
                allow_loose_object_property_names: false,
                allow_missing_commas: false,
                allow_single_quoted_strings: false,
                allow_hexadecimal_numbers: false,
                allow_unary_plus_numbers: false,
                allow_bare_decimal_point_numbers: false,
                allow_non_finite_numbers: false,
                allow_extended_string_escapes: false,
            },
        )
        .map_err(|_| "config.jsonc contains invalid JSON.".to_string())?
        .unwrap_or_else(|| serde_json::json!({})),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => serde_json::json!({}),
        Err(error) => return Err(format!("cannot read config.jsonc: {error}")),
    };
    if let Some(raw) = env
        .get("SWEEP_CONFIG")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        let overlay: Value = serde_json::from_str(raw)
            .map_err(|_| "SWEEP_CONFIG contains invalid JSON.".to_string())?;
        let mut merged = config.as_object().cloned().unwrap_or_default();
        if let Some(overlay) = overlay.as_object() {
            merged.extend(overlay.clone());
        }
        config = Value::Object(merged);
    }
    Ok(config)
}
pub fn resolve_provider(
    config: &Value,
    env: &HashMap<String, String>,
) -> Result<ResolvedProvider, String> {
    let name = config
        .get("defaultProvider")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or("no LLM configured.")?;
    let entry = config
        .get("providers")
        .and_then(|p| p.get(name))
        .filter(|p| p.is_object())
        .ok_or_else(|| format!("provider \"{name}\" not found in config."))?;
    let field = |key| {
        entry
            .get(key)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    let model = field("model");
    let base_url = field("baseURL").or_else(|| {
        match name {
            "openai" => env.get("OPENAI_BASE_URL"),
            "anthropic" => env.get("ANTHROPIC_BASE_URL"),
            _ => None,
        }
        .cloned()
    });
    let key = field("apiKey");
    match name {
        "groq" | "mistral" | "ollama" if base_url.is_none() => {
            return Err(format!("provider \"{name}\" requires baseURL."));
        }
        "anthropic" | "openai" | "openrouter" | "groq" | "mistral" | "ollama" | "claude-code" => {}
        _ if base_url.is_none() || key.is_none() || model.is_none() => {
            return Err(format!(
                "provider \"{name}\" requires baseURL, apiKey, and model."
            ));
        }
        _ => {}
    }
    if name == "test" {
        return Err("test provider has no responses configured.".into());
    }
    if model.is_none() && name != "claude-code" {
        return Err(format!("provider \"{name}\" has no model set in config."));
    }
    let api_key = match key {
        Some(key) if key.starts_with('$') => Some(
            env.get(&key[1..])
                .filter(|s| !s.is_empty())
                .cloned()
                .ok_or_else(|| format!("environment variable {} is not set.", &key[1..]))?,
        ),
        Some(key) => Some(key),
        None => match name {
            "anthropic" => env.get("ANTHROPIC_API_KEY"),
            "openai" => env.get("OPENAI_API_KEY"),
            "openrouter" => env.get("OPENROUTER_API_KEY"),
            _ => None,
        }
        .cloned(),
    };
    Ok(ResolvedProvider {
        name: name.into(),
        model,
        api_key,
        base_url,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn jsonc_and_shallow_override_preserve_unknown_fields() {
        let home = tempfile::tempdir().unwrap();
        std::fs::write(
            home.path().join("config.jsonc"),
            r#"{// comment
"defaultProvider":"anthropic","providers":{"anthropic":{"model":"old"}},"future":true,}"#,
        )
        .unwrap();
        let env = HashMap::from([(
            "SWEEP_CONFIG".into(),
            r#"{"providers":{"ollama":{"model":"local"}}}"#.into(),
        )]);
        assert_eq!(
            load(home.path(), &env).unwrap(),
            json!({"defaultProvider":"anthropic","providers":{"ollama":{"model":"local"}},"future":true})
        );
    }
    #[test]
    fn malformed_sources_name_source_without_exposing_content() {
        let home = tempfile::tempdir().unwrap();
        std::fs::write(home.path().join("config.jsonc"), "{secret").unwrap();
        assert_eq!(
            load(home.path(), &HashMap::new()).unwrap_err(),
            "config.jsonc contains invalid JSON."
        );
        std::fs::remove_file(home.path().join("config.jsonc")).unwrap();
        assert_eq!(
            load(
                home.path(),
                &HashMap::from([("SWEEP_CONFIG".into(), "secret".into())])
            )
            .unwrap_err(),
            "SWEEP_CONFIG contains invalid JSON."
        );
    }
    #[test]
    fn resolves_env_keys_and_cli_optional_model() {
        let env = HashMap::from([("MY_KEY".into(), "secret".into())]);
        let c = json!({"defaultProvider":"custom","providers":{"custom":{"baseURL":"http://localhost/v1","model":"m","apiKey":"$MY_KEY"}}});
        assert_eq!(
            resolve_provider(&c, &env).unwrap().api_key.as_deref(),
            Some("secret")
        );
        assert!(resolve_provider(&c, &HashMap::new()).is_err());
        let c = json!({"defaultProvider":"claude-code","providers":{"claude-code":{}}});
        assert!(resolve_provider(&c, &env).unwrap().model.is_none());
    }
    #[test]
    fn compatibility_providers_require_endpoint_and_unknowns_require_key() {
        for name in ["groq", "mistral", "ollama", "custom"] {
            let c = json!({"defaultProvider":name,"providers":{name:{"model":"m"}}});
            assert!(resolve_provider(&c, &HashMap::new()).is_err());
        }
    }
    #[test]
    fn jsonc_does_not_silently_accept_json5_or_missing_commas() {
        let home = tempfile::tempdir().unwrap();
        for raw in [
            r#"{key:"value"}"#,
            r#"{"key":'value'}"#,
            r#"{"a":1 "b":2}"#,
            r#"{"a":0xff}"#,
        ] {
            std::fs::write(home.path().join("config.jsonc"), raw).unwrap();
            assert!(
                load(home.path(), &HashMap::new()).is_err(),
                "accepted {raw}"
            );
        }
    }
    #[test]
    fn provider_endpoints_honor_legacy_env_defaults_but_explicit_config_wins() {
        for (name, var) in [
            ("openai", "OPENAI_BASE_URL"),
            ("anthropic", "ANTHROPIC_BASE_URL"),
        ] {
            let env = HashMap::from([(var.into(), "https://proxy.example/v1".into())]);
            let mut config = json!({"defaultProvider":name,"providers":{name:{"model":"m"}}});
            assert_eq!(
                resolve_provider(&config, &env).unwrap().base_url.as_deref(),
                Some("https://proxy.example/v1")
            );
            config["providers"][name]["baseURL"] = json!("https://explicit.example/v1");
            assert_eq!(
                resolve_provider(&config, &env).unwrap().base_url.as_deref(),
                Some("https://explicit.example/v1")
            );
        }
    }
}
