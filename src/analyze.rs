use crate::config::ResolvedProvider;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashMap, path::Path};
use tokio_util::sync::CancellationToken;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Clear,
    Caution,
    Danger,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Behavior {
    pub description: String,
    pub sudo: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AnalysisPass {
    Ok {
        severity: Severity,
        summary: String,
        flags: Vec<String>,
        behaviors: Vec<Behavior>,
    },
    Failed {
        reason: String,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManipulationPass {
    Clean,
    Fired,
    Failed { reason: String },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AnalysisResult {
    NoProvider,
    Analyzed {
        analysis: AnalysisPass,
        manipulation: ManipulationPass,
    },
}
#[derive(Clone, Debug)]
pub enum AnalysisProvider {
    None,
    Broken(String),
    Test {
        analysis: Value,
        manipulation: Value,
    },
    Real(ResolvedProvider),
}
#[derive(Clone, Debug)]
pub struct AnalysisInput {
    pub url: String,
    pub final_url: Option<String>,
    pub script_bytes: Vec<u8>,
    pub redacted_command: Option<String>,
}
pub fn resolve_analysis_provider(home: &Path, env: &HashMap<String, String>) -> AnalysisProvider {
    if let Some(raw) = env
        .get("SWEEP_TEST_RESPONSES")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        return match serde_json::from_str::<Value>(raw) {
            Ok(value)
                if value.is_object()
                    && value.get("analysis").is_some()
                    && value.get("manipulation").is_some() =>
            {
                let analysis = value["analysis"].clone();
                let manipulation = value["manipulation"].clone();
                if [&analysis, &manipulation]
                    .iter()
                    .any(|v| v.as_array().is_some_and(Vec::is_empty))
                {
                    AnalysisProvider::Broken("SWEEP_TEST_RESPONSES analysis/manipulation must have at least one response".into())
                } else {
                    AnalysisProvider::Test {
                        analysis,
                        manipulation,
                    }
                }
            }
            _ => AnalysisProvider::Broken(
                "SWEEP_TEST_RESPONSES must be a JSON { analysis, manipulation } object".into(),
            ),
        };
    }
    match crate::config::load(home, env)
        .and_then(|config| crate::config::resolve_provider(&config, env))
    {
        Ok(provider) => AnalysisProvider::Real(provider),
        Err(_) => AnalysisProvider::None,
    }
}

#[derive(Deserialize)]
struct AnalysisFields {
    behaviors: Vec<Behavior>,
    flags: Vec<String>,
    severity: Severity,
    summary: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManipulationFields {
    manipulation_detected: bool,
}

pub async fn analyze_script(
    input: AnalysisInput,
    provider: AnalysisProvider,
    cancel: CancellationToken,
) -> AnalysisResult {
    if matches!(provider, AnalysisProvider::None) {
        return AnalysisResult::NoProvider;
    }
    if let AnalysisProvider::Broken(reason) = provider {
        return AnalysisResult::Analyzed {
            analysis: AnalysisPass::Failed {
                reason: reason.clone(),
            },
            manipulation: ManipulationPass::Failed { reason },
        };
    }
    let content = user_content(&input);
    let analysis = run_pass::<AnalysisFields>(&provider, false, &content, &cancel);
    let manipulation = run_pass::<ManipulationFields>(&provider, true, &content, &cancel);
    let (analysis, manipulation) = tokio::join!(analysis, manipulation);
    AnalysisResult::Analyzed {
        analysis: match analysis {
            Ok(a) => AnalysisPass::Ok {
                severity: a.severity,
                summary: a.summary,
                flags: a.flags,
                behaviors: a.behaviors,
            },
            Err(reason) => AnalysisPass::Failed { reason },
        },
        manipulation: match manipulation {
            Ok(m) if m.manipulation_detected => ManipulationPass::Fired,
            Ok(_) => ManipulationPass::Clean,
            Err(reason) => ManipulationPass::Failed { reason },
        },
    }
}
fn user_content(input: &AnalysisInput) -> String {
    let normalize = |s: &str| {
        url::Url::parse(s)
            .map(|mut u| {
                u.set_fragment(None);
                u.to_string()
            })
            .unwrap_or_else(|_| s.into())
    };
    let provenance = match &input.final_url {
        Some(final_url) if normalize(&input.url) != normalize(final_url) => format!(
            "Install script requested from {}; after an HTTP redirect, served from {}",
            input.url, final_url
        ),
        _ => format!("Install script fetched from {}", input.url),
    };
    let command = input
        .redacted_command
        .as_ref()
        .map(|cmd| format!("\nCommand the user is about to run: `{cmd}`"))
        .unwrap_or_default();
    format!(
        "{provenance}{command}\n\nScript contents:\n{}",
        String::from_utf8_lossy(&input.script_bytes)
    )
}
fn prompt(manipulation: bool) -> (String, Value) {
    let (instructions, schema) = if manipulation {
        (
            include_str!("prompts/manipulation.txt"),
            include_str!("prompts/manipulation.schema.json"),
        )
    } else {
        (
            include_str!("prompts/analysis.txt"),
            include_str!("prompts/analysis.schema.json"),
        )
    };
    (
        format!(
            "{instructions}\n\nReturn a JSON object matching this schema:\n{schema}\nOutput only the JSON object — no prose before or after it, no markdown code fences."
        ),
        serde_json::from_str(schema).expect("embedded response schema"),
    )
}
const RETRY: &str = "Respond ONLY with valid JSON matching the schema. No markdown fences, no comments, no text outside the JSON object.";
fn parse_reply<T: serde::de::DeserializeOwned>(raw: &str) -> Result<T, String> {
    let raw = raw.trim();
    let unfenced = raw
        .strip_prefix("```")
        .and_then(|s| s.split_once('\n'))
        .and_then(|(_, s)| s.strip_suffix("```"))
        .filter(|s| !s.contains("```"));
    let value: Value = serde_json::from_str(unfenced.unwrap_or(raw).trim())
        .map_err(|_| "Model response was not valid JSON.".to_string())?;
    serde_json::from_value(value)
        .map_err(|_| "Model response did not match the expected schema.".to_string())
}
async fn run_pass<T: serde::de::DeserializeOwned>(
    provider: &AnalysisProvider,
    manipulation: bool,
    content: &str,
    cancel: &CancellationToken,
) -> Result<T, String> {
    if cancel.is_cancelled() {
        return Err("Analysis cancelled.".into());
    }
    let work = async {
        let (system, schema) = prompt(manipulation);
        let mut messages = vec![serde_json::json!({"role":"user","content":content})];
        for attempt in 0..2 {
            let raw = match provider {
                AnalysisProvider::Test {
                    analysis,
                    manipulation: response,
                } => {
                    let responses = if manipulation { response } else { analysis };
                    let value = if let Some(list) = responses.as_array() {
                        list.get(attempt).ok_or_else(|| {
                            format!(
                                "No test response left for call {} ({} provided).",
                                attempt + 1,
                                list.len()
                            )
                        })?
                    } else {
                        responses
                    };
                    let raw = value
                        .as_str()
                        .map(str::to_string)
                        .unwrap_or_else(|| value.to_string());
                    if let Some(error) = raw.strip_prefix("ERROR:") {
                        return Err(error.trim_start().to_string());
                    }
                    raw
                }
                AnalysisProvider::Real(config) => {
                    call_provider(config, &system, &messages, &schema).await?
                }
                _ => return Err("No analysis provider.".into()),
            };
            match parse_reply(&raw) {
                Ok(value) => return Ok(value),
                Err(error) if attempt == 1 => return Err(error),
                Err(_) => {
                    messages.push(serde_json::json!({"role":"assistant","content":raw}));
                    messages.push(serde_json::json!({"role":"user","content":RETRY}));
                }
            }
        }
        unreachable!()
    };
    tokio::select! {
        biased;
        _=cancel.cancelled()=>Err("Analysis cancelled.".into()),
        result=tokio::time::timeout(std::time::Duration::from_secs(60),work)=>result.unwrap_or_else(|_|Err("Analysis timed out.".into())),
    }
}
async fn call_provider(
    config: &ResolvedProvider,
    system: &str,
    messages: &[Value],
    schema: &Value,
) -> Result<String, String> {
    if config.name == "claude-code" {
        return call_cli(config, system, messages, schema, Path::new("claude")).await;
    }
    if matches!(config.name.as_str(), "anthropic" | "openai" | "openrouter")
        && config.api_key.is_none()
    {
        return Err(format!("{} API key is missing.", config.name));
    }
    let base = match config.base_url.as_deref() {
        Some(base) => base,
        None => match config.name.as_str() {
            "anthropic" => "https://api.anthropic.com/v1",
            "openai" => "https://api.openai.com/v1",
            "openrouter" => "https://openrouter.ai/api/v1",
            _ => return Err("Provider requires baseURL.".into()),
        },
    };
    let mut body = serde_json::json!({"model":config.model});
    let path = match config.name.as_str() {
        "anthropic" => {
            body["system"] = Value::String(system.into());
            body["messages"] = Value::Array(messages.to_vec());
            body["max_tokens"] = Value::from(anthropic_output_limit(
                config.model.as_deref().unwrap_or_default(),
            ));
            body["tools"] = serde_json::json!([{"name":"sweep_analysis","description":"Return the requested structured analysis.","input_schema":schema}]);
            body["tool_choice"] = serde_json::json!({"type":"tool","name":"sweep_analysis"});
            "messages"
        }
        "openai" => {
            body["instructions"] = Value::String(system.into());
            body["input"] = Value::Array(messages.to_vec());
            body["text"] = serde_json::json!({"format":{"type":"json_schema","name":"sweep_analysis","schema":schema,"strict":true}});
            "responses"
        }
        _ => {
            let mut chat = vec![serde_json::json!({"role":"system","content":system})];
            chat.extend_from_slice(messages);
            body["messages"] = Value::Array(chat);
            body["response_format"] = if matches!(
                config.name.as_str(),
                "openrouter" | "groq" | "mistral" | "ollama"
            ) {
                let mut format = serde_json::json!({"type":"json_schema","json_schema":{"name":"sweep_analysis","schema":schema}});
                if config.name != "openrouter" {
                    format["json_schema"]["strict"] = Value::Bool(true);
                }
                format
            } else {
                serde_json::json!({"type":"json_object"})
            };
            "chat/completions"
        }
    };
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(20))
        .build()
        .map_err(|e| e.to_string())?;
    let mut request = client
        .post(format!("{}/{path}", base.trim_end_matches('/')))
        .json(&body);
    if config.name == "anthropic" {
        request = request.header("anthropic-version", "2023-06-01");
        if let Some(key) = &config.api_key {
            request = request.header("x-api-key", key);
        }
    } else {
        request = request.bearer_auth(config.api_key.as_deref().unwrap_or("nokey"));
    }
    let mut attempt = 0;
    let response = loop {
        let result = request
            .try_clone()
            .expect("JSON request is clonable")
            .send()
            .await;
        let backoff = std::time::Duration::from_millis(2000_u64 << attempt);
        match result {
            Ok(response)
                if attempt < 2
                    && (matches!(response.status().as_u16(), 408 | 409 | 429)
                        || response.status().is_server_error()) =>
            {
                let delay = retry_delay(response.headers(), backoff);
                drop(response);
                tokio::time::sleep(delay).await;
            }
            Ok(response) => break response,
            Err(error) if attempt < 2 && (error.is_connect() || error.is_timeout()) => {
                tokio::time::sleep(backoff).await
            }
            Err(error) => return Err(error.to_string()),
        }
        attempt += 1;
    };
    let status = response.status();
    // Bound error details and scrub credentials before they can reach the screen.
    let raw = response.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        let scrubbed = if let Some(key) = &config.api_key {
            raw.replace(key, "<redacted>")
        } else {
            raw
        };
        let detail = scrubbed.chars().take(500).collect::<String>();
        return Err(format!("Provider returned HTTP {status}: {detail}"));
    }
    let value: Value = serde_json::from_str(&raw)
        .map_err(|_| "Provider response was not valid JSON.".to_string())?;
    match config.name.as_str() {
        "anthropic" => {
            let content = value["content"]
                .as_array()
                .ok_or("Model returned no structured output.")?;
            if let Some(tool) = content
                .iter()
                .find(|item| item["type"] == "tool_use" && item["name"] == "sweep_analysis")
            {
                return Ok(tool["input"].to_string());
            }
            content
                .iter()
                .filter_map(|item| item["text"].as_str())
                .reduce(|_, last| last)
                .map(str::to_string)
                .ok_or("Model returned no structured output.".into())
        }
        "openai" => value["output"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|item| item["content"].as_array())
            .flatten()
            .filter(|item| item["type"] == "output_text")
            .filter_map(|item| item["text"].as_str())
            .map(str::to_string)
            .reduce(|a, b| a + &b)
            .ok_or("Model returned no structured output.".into()),
        _ => value["choices"][0]["message"]["content"]
            .as_str()
            .map(str::to_string)
            .ok_or("Model returned no structured output.".into()),
    }
}
fn anthropic_output_limit(model: &str) -> u64 {
    // Model families have different output ceilings. Unknown/proxy models use
    // a conservative budget to avoid rejecting otherwise valid requests.
    if [
        "claude-opus-4-8",
        "claude-opus-4-7",
        "claude-fable-5",
        "claude-sonnet-4-6",
        "claude-opus-4-6",
    ]
    .iter()
    .any(|family| model.contains(family))
    {
        128000
    } else if ["claude-sonnet-4-5", "claude-opus-4-5", "claude-haiku-4-5"]
        .iter()
        .any(|family| model.contains(family))
        || model.contains("claude-sonnet-4-")
    {
        64000
    } else if model.contains("claude-opus-4-") {
        32000
    } else {
        4096
    }
}
fn retry_delay(
    headers: &reqwest::header::HeaderMap,
    backoff: std::time::Duration,
) -> std::time::Duration {
    let numeric = |key: &str| {
        headers
            .get(key)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<f64>().ok())
    };
    let milliseconds = numeric("retry-after-ms")
        .or_else(|| numeric("retry-after").map(|s| s * 1000.0))
        .or_else(|| {
            headers
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| chrono::DateTime::parse_from_rfc2822(v).ok())
                .map(|date| {
                    (date.timestamp_millis() - chrono::Utc::now().timestamp_millis()) as f64
                })
        });
    milliseconds
        .filter(|n| n.is_finite() && *n >= 0.0 && (*n < 60000.0 || *n < backoff.as_millis() as f64))
        .map(|n| std::time::Duration::from_secs_f64(n / 1000.0))
        .unwrap_or(backoff)
}
async fn call_cli(
    config: &ResolvedProvider,
    system: &str,
    messages: &[Value],
    schema: &Value,
    program: &Path,
) -> Result<String, String> {
    use std::process::Stdio;
    use tokio::io::AsyncWriteExt;
    let mut command = tokio::process::Command::new(program);
    command.args(["--tools", "", "--system-prompt", system]);
    if let Some(model) = &config.model {
        command.args(["--model", model]);
    }
    command
        .args([
            "--no-session-persistence",
            "--json-schema",
            &schema.to_string(),
            "-p",
        ])
        .current_dir(std::env::temp_dir())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command
        .spawn()
        .map_err(|e| format!("cannot start claude: {e}"))?;
    let content = messages
        .iter()
        .map(|m| {
            format!(
                "{}: {}",
                if m["role"] == "assistant" {
                    "Assistant"
                } else {
                    "User"
                },
                m["content"].as_str().unwrap_or("")
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    let mut stdin = child.stdin.take().ok_or("cannot open claude stdin")?;
    // Drain stdout/stderr concurrently with writing so a verbose CLI cannot deadlock.
    let write = async move {
        stdin.write_all(content.as_bytes()).await?;
        stdin.shutdown().await
    };
    let (written, output) = tokio::join!(write, child.wait_with_output());
    let output = output.map_err(|e| e.to_string())?;
    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let error = if error.is_empty() {
            format!(
                "claude exited with code {}.",
                output.status.code().unwrap_or(1)
            )
        } else {
            error
        };
        return Err(if let Some(key) = &config.api_key {
            error.replace(key, "<redacted>")
        } else {
            error
        });
    }
    written.map_err(|e| format!("cannot write claude stdin: {e}"))?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn input() -> AnalysisInput {
        AnalysisInput {
            url: "https://example.com/install".into(),
            final_url: None,
            script_bytes: b"echo hello".to_vec(),
            redacted_command: None,
        }
    }
    fn valid() -> Value {
        json!({"behaviors":[{"description":"Prints hello","sudo":false}],"flags":[],"severity":"clear","summary":"Simple script."})
    }
    #[tokio::test]
    async fn parses_each_pass_independently_and_retries_only_invalid_output() {
        let result = analyze_script(
            input(),
            AnalysisProvider::Test {
                analysis: json!(["bad json", valid()]),
                manipulation: json!({"manipulationDetected":true}),
            },
            CancellationToken::new(),
        )
        .await;
        assert!(matches!(
            result,
            AnalysisResult::Analyzed {
                analysis: AnalysisPass::Ok {
                    severity: Severity::Clear,
                    ..
                },
                manipulation: ManipulationPass::Fired
            }
        ));
        let result = analyze_script(
            input(),
            AnalysisProvider::Test {
                analysis: valid(),
                manipulation: json!("ERROR: unavailable"),
            },
            CancellationToken::new(),
        )
        .await;
        assert!(matches!(
            result,
            AnalysisResult::Analyzed {
                analysis: AnalysisPass::Ok { .. },
                manipulation: ManipulationPass::Failed { .. }
            }
        ));
    }
    #[test]
    fn broken_canned_seam_wins_over_invalid_config() {
        let home = tempfile::tempdir().unwrap();
        std::fs::write(home.path().join("config.jsonc"), "bad").unwrap();
        for canned in ["[]", r#"{"analysis":[],"manipulation":{}}"#, "bad"] {
            let env = HashMap::from([("SWEEP_TEST_RESPONSES".into(), canned.into())]);
            assert!(matches!(
                resolve_analysis_provider(home.path(), &env),
                AnalysisProvider::Broken(_)
            ));
        }
    }
    #[tokio::test]
    async fn cancellation_never_returns_trusted_analysis() {
        let cancel = CancellationToken::new();
        cancel.cancel();
        let result = analyze_script(
            input(),
            AnalysisProvider::Test {
                analysis: valid(),
                manipulation: json!({"manipulationDetected":false}),
            },
            cancel,
        )
        .await;
        assert!(matches!(
            result,
            AnalysisResult::Analyzed {
                analysis: AnalysisPass::Failed { .. },
                manipulation: ManipulationPass::Failed { .. }
            }
        ));
    }

    #[tokio::test]
    async fn live_protocols_send_isolated_passes_and_provenance() {
        for name in ["anthropic", "openai", "openrouter", "ollama", "custom"] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let base = format!("http://{}/v1", listener.local_addr().unwrap());
            let name_owned = name.to_string();
            let server = tokio::spawn(async move {
                let mut requests = Vec::new();
                for _ in 0..2 {
                    let (mut stream, _) = listener.accept().await.unwrap();
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                    let mut bytes = Vec::new();
                    let mut buf = [0; 4096];
                    let header_end;
                    loop {
                        let n = stream.read(&mut buf).await.unwrap();
                        assert!(n > 0);
                        bytes.extend_from_slice(&buf[..n]);
                        if let Some(end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                            header_end = end + 4;
                            break;
                        }
                    }
                    let headers = String::from_utf8_lossy(&bytes[..header_end]).to_string();
                    let len: usize = headers
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .map(|x| x.trim().parse().unwrap())
                        })
                        .unwrap();
                    while bytes.len() < header_end + len {
                        let n = stream.read(&mut buf).await.unwrap();
                        assert!(n > 0);
                        bytes.extend_from_slice(&buf[..n]);
                    }
                    let body: Value =
                        serde_json::from_slice(&bytes[header_end..header_end + len]).unwrap();
                    let manipulation = body.to_string().contains("security reviewer");
                    let reply = if manipulation {
                        json!({"manipulationDetected":false})
                    } else {
                        valid()
                    };
                    let response=match name_owned.as_str(){
    "anthropic"=>json!({"content":[{"type":"tool_use","name":"sweep_analysis","input":reply}]}),
    "openai"=>json!({"output":[{"type":"message","content":[{"type":"output_text","text":reply.to_string()}]}]}),
    _=>json!({"choices":[{"message":{"content":reply.to_string()}}]})
   }.to_string();
                    stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",response.len(),response).as_bytes()).await.unwrap();
                    requests.push((headers, body));
                }
                requests
            });
            let mut args = input();
            args.final_url = Some("https://cdn.example.net/script".into());
            args.redacted_command =
                Some("TOKEN=<redacted> curl https://example.com/install | sudo sh".into());
            let result = analyze_script(
                args,
                AnalysisProvider::Real(ResolvedProvider {
                    name: name.into(),
                    model: Some("fixture-model".into()),
                    api_key: Some("fixture-key".into()),
                    base_url: Some(base),
                }),
                CancellationToken::new(),
            )
            .await;
            assert!(
                matches!(
                    result,
                    AnalysisResult::Analyzed {
                        analysis: AnalysisPass::Ok { .. },
                        manipulation: ManipulationPass::Clean
                    }
                ),
                "{name}: {result:?}"
            );
            let requests = server.await.unwrap();
            for (headers, body) in requests {
                let text = body.to_string();
                assert!(text.contains(
                    "after an HTTP redirect, served from https://cdn.example.net/script"
                ));
                assert!(text.contains("TOKEN=<redacted>"));
                assert!(text.contains("Script contents:"));
                assert!(headers.contains("fixture-key"));
                match name {
                    "anthropic" => {
                        assert!(headers.starts_with("POST /v1/messages"));
                        assert_eq!(body["tool_choice"]["type"], "tool");
                        assert_eq!(body["messages"].as_array().unwrap().len(), 1);
                    }
                    "openai" => {
                        assert!(headers.starts_with("POST /v1/responses"));
                        assert_eq!(body["text"]["format"]["type"], "json_schema");
                    }
                    _ => {
                        assert!(headers.starts_with("POST /v1/chat/completions"));
                        assert_eq!(body["messages"].as_array().unwrap().len(), 2);
                        assert_eq!(
                            body["response_format"]["type"],
                            if name == "custom" {
                                "json_object"
                            } else {
                                "json_schema"
                            }
                        );
                    }
                }
            }
        }
    }

    #[tokio::test]
    async fn cli_preserves_isolation_and_stdin_and_reaps_failed_calls() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let program = dir.path().join("fake-claude");
        let log = dir.path().join("request");
        std::fs::write(
            &program,
            format!(
                r#"#!/bin/sh
printf '%s\n' "$PWD" "$@" > '{}'
cat >> '{}'
printf '%s' '{{"manipulationDetected":false}}'
"#,
                log.display(),
                log.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        let config = ResolvedProvider {
            name: "claude-code".into(),
            model: Some("test-model".into()),
            api_key: None,
            base_url: None,
        };
        let reply = call_cli(
            &config,
            "review system",
            &[
                json!({"role":"user","content":"first"}),
                json!({"role":"assistant","content":"bad json"}),
                json!({"role":"user","content":RETRY}),
            ],
            &json!({"type":"object"}),
            &program,
        )
        .await
        .unwrap();
        assert_eq!(reply, r#"{"manipulationDetected":false}"#);
        let request = std::fs::read_to_string(log).unwrap();
        assert!(request.contains("--no-session-persistence"));
        assert!(request.contains("--tools\n\n"));
        assert!(request.contains("--model\ntest-model"));
        assert!(request.contains("User: first\n\nAssistant: bad json\n\nUser: Respond ONLY"));
        assert!(!request.starts_with(&std::env::current_dir().unwrap().display().to_string()));
        std::fs::write(&program, "#!/bin/sh\necho 'CLI auth failed' >&2\nexit 9\n").unwrap();
        assert_eq!(
            call_cli(&config, "review", &[], &json!({}), &program)
                .await
                .unwrap_err(),
            "CLI auth failed"
        );
    }
    #[tokio::test]
    async fn live_passes_start_concurrently_and_cancel_without_waiting_for_server() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let cancel = CancellationToken::new();
        let signal = cancel.clone();
        let server = tokio::spawn(async move {
            let first = listener.accept().await.unwrap();
            let second = listener.accept().await.unwrap();
            signal.cancel();
            (first, second)
        });
        let provider = AnalysisProvider::Real(ResolvedProvider {
            name: "openai".into(),
            model: Some("model".into()),
            api_key: Some("fixture-key".into()),
            base_url: Some(base),
        });
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            analyze_script(input(), provider, cancel),
        )
        .await
        .unwrap();
        assert!(matches!(
            result,
            AnalysisResult::Analyzed {
                analysis: AnalysisPass::Failed { .. },
                manipulation: ManipulationPass::Failed { .. }
            }
        ));
        server.await.unwrap();
    }

    #[tokio::test]
    async fn cancelling_cli_work_terminates_the_subprocess() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let program = dir.path().join("slow-claude");
        let pid_file = dir.path().join("pid");
        std::fs::write(
            &program,
            format!(
                "#!/bin/sh\necho $$ > '{}'\nexec sleep 60\n",
                pid_file.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        let task = tokio::spawn(async move {
            call_cli(
                &ResolvedProvider {
                    name: "claude-code".into(),
                    model: None,
                    api_key: None,
                    base_url: None,
                },
                "review",
                &[],
                &json!({}),
                &program,
            )
            .await
        });
        let pid = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                if let Ok(pid) = std::fs::read_to_string(&pid_file) {
                    break pid.trim().to_string();
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                let alive = tokio::process::Command::new("kill")
                    .args(["-0", &pid])
                    .stderr(std::process::Stdio::null())
                    .status()
                    .await
                    .unwrap()
                    .success();
                if !alive {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
    }
    #[tokio::test]
    async fn schema_failures_are_untrusted_and_fenced_json_is_accepted() {
        let fenced = format!("```json\n{}\n```", valid());
        let result = analyze_script(
            input(),
            AnalysisProvider::Test {
                analysis: json!(fenced),
                manipulation: json!({"manipulationDetected":"false"}),
            },
            CancellationToken::new(),
        )
        .await;
        assert!(matches!(
            result,
            AnalysisResult::Analyzed {
                analysis: AnalysisPass::Ok { .. },
                manipulation: ManipulationPass::Failed { .. }
            }
        ));
        let result = analyze_script(
            input(),
            AnalysisProvider::Test {
                analysis: json!({"severity":"safe"}),
                manipulation: json!({"manipulationDetected":false}),
            },
            CancellationToken::new(),
        )
        .await;
        assert!(matches!(
            result,
            AnalysisResult::Analyzed {
                analysis: AnalysisPass::Failed { .. },
                manipulation: ManipulationPass::Clean
            }
        ));
    }
    #[test]
    fn cosmetic_url_normalization_does_not_claim_redirect_or_add_absent_command() {
        let mut args = input();
        args.url = "https://EXAMPLE.com:443/install#local".into();
        args.final_url = Some("https://example.com/install".into());
        let text = user_content(&args);
        assert!(!text.contains("redirect"));
        assert!(!text.contains("Command the user"));
        assert!(text.ends_with("\n\nScript contents:\necho hello"));
    }

    async fn read_request(stream: &mut tokio::net::TcpStream) -> Value {
        use tokio::io::AsyncReadExt;
        let mut bytes = Vec::new();
        let mut buf = [0; 4096];
        let header_end;
        loop {
            let n = stream.read(&mut buf).await.unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&buf[..n]);
            if let Some(end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                header_end = end + 4;
                break;
            }
        }
        let headers = String::from_utf8_lossy(&bytes[..header_end]);
        let len: usize = headers
            .lines()
            .find_map(|l| {
                l.to_ascii_lowercase()
                    .strip_prefix("content-length:")
                    .map(|n| n.trim().parse().unwrap())
            })
            .unwrap();
        while bytes.len() < header_end + len {
            let n = stream.read(&mut buf).await.unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&buf[..n]);
        }
        serde_json::from_slice(&bytes[header_end..header_end + len]).unwrap()
    }
    #[tokio::test]
    async fn transient_http_failures_retry_twice_without_changing_the_conversation() {
        use tokio::io::AsyncWriteExt;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let mut requests = Vec::new();
            for status in [503, 429, 200] {
                let (mut stream, _) = listener.accept().await.unwrap();
                requests.push(read_request(&mut stream).await);
                let body = if status == 200 {
                    json!({"output":[{"content":[{"type":"output_text","text":"{}"}]}]}).to_string()
                } else {
                    "busy".into()
                };
                stream.write_all(format!("HTTP/1.1 {status} fixture\r\nRetry-After: 0\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            }
            requests
        });
        let config = ResolvedProvider {
            name: "openai".into(),
            model: Some("m".into()),
            api_key: Some("fixture-key".into()),
            base_url: Some(base),
        };
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            call_provider(
                &config,
                "system",
                &[json!({"role":"user","content":"script"})],
                &json!({"type":"object"}),
            ),
        )
        .await
        .unwrap();
        assert_eq!(result.unwrap(), "{}");
        let requests = server.await.unwrap();
        assert_eq!(requests[0], requests[1]);
        assert_eq!(requests[1], requests[2]);
    }
    #[tokio::test]
    async fn auth_errors_are_not_retried_and_never_leak_boundary_cut_credentials() {
        use tokio::io::AsyncWriteExt;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let key = "SECRET".repeat(100);
        let body = key.clone();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            read_request(&mut stream).await;
            stream.write_all(format!("HTTP/1.1 401 Unauthorized\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            assert!(
                tokio::time::timeout(std::time::Duration::from_millis(100), listener.accept())
                    .await
                    .is_err()
            );
        });
        let config = ResolvedProvider {
            name: "openai".into(),
            model: Some("m".into()),
            api_key: Some(key),
            base_url: Some(base),
        };
        let error = call_provider(&config, "system", &[], &json!({}))
            .await
            .unwrap_err();
        assert!(!error.contains("SECRET"), "credential fragment leaked");
        assert!(error.contains("401"));
        server.await.unwrap();
    }

    #[tokio::test]
    async fn anthropic_requests_preserve_model_output_limits() {
        use tokio::io::AsyncWriteExt;
        for (model, limit) in [
            ("claude-3-haiku-20240307", 4096),
            ("proxy-model", 4096),
            ("claude-sonnet-4-20250514", 64000),
            ("claude-opus-4-20250514", 32000),
            ("claude-opus-4-1-20250805", 32000),
            ("claude-haiku-4-5", 64000),
            ("claude-sonnet-4-5", 64000),
            ("claude-opus-4-5", 64000),
            ("claude-sonnet-4-6", 128000),
            ("claude-opus-4-6", 128000),
            ("claude-opus-4-7", 128000),
            ("claude-opus-4-8", 128000),
            ("claude-fable-5", 128000),
        ] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let base = format!("http://{}", listener.local_addr().unwrap());
            let server = tokio::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();
                let request = read_request(&mut stream).await;
                let requested = request["max_tokens"].as_u64().unwrap();
                let (status, body) = if requested > limit {
                    (400, "max_tokens exceeds model output limit".to_string())
                } else {
                    (
                        200,
                        json!({"content":[{"type":"tool_use","name":"sweep_analysis","input":{}}]})
                            .to_string(),
                    )
                };
                stream.write_all(format!("HTTP/1.1 {status} fixture\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
                requested
            });
            let config = ResolvedProvider {
                name: "anthropic".into(),
                model: Some(model.into()),
                api_key: Some("fixture-key".into()),
                base_url: Some(base),
            };
            let result = call_provider(&config, "system", &[], &json!({})).await;
            let requested = server.await.unwrap();
            assert_eq!(
                result.unwrap_or_else(|error| panic!("{model}: {error}")),
                "{}"
            );
            assert_eq!(
                requested, limit,
                "use the supported output budget for {model}"
            );
        }
    }

    #[tokio::test]
    async fn official_provider_without_key_fails_before_transmitting_script() {
        for name in ["anthropic", "openai", "openrouter"] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let base = format!("http://{}", listener.local_addr().unwrap());
            let config = ResolvedProvider {
                name: name.into(),
                model: Some("m".into()),
                api_key: None,
                base_url: Some(base),
            };
            let result = tokio::time::timeout(
                std::time::Duration::from_millis(100),
                call_provider(
                    &config,
                    "system",
                    &[json!({"role":"user","content":"private script"})],
                    &json!({}),
                ),
            )
            .await;
            assert!(
                matches!(result, Ok(Err(_))),
                "{name} must reject absent credentials locally"
            );
            assert!(
                tokio::time::timeout(std::time::Duration::from_millis(20), listener.accept())
                    .await
                    .is_err()
            );
        }
    }

    #[tokio::test]
    async fn provider_endpoint_redirect_preserves_post_body() {
        use tokio::io::AsyncWriteExt;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let first = read_request(&mut stream).await;
            stream.write_all(b"HTTP/1.1 307 Temporary Redirect\r\nLocation: /actual\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await.unwrap();
            let (mut stream, _) = listener.accept().await.unwrap();
            let second = read_request(&mut stream).await;
            let body =
                json!({"output":[{"content":[{"type":"output_text","text":"{}"}]}]}).to_string();
            stream
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
            assert_eq!(first, second);
        });
        let config = ResolvedProvider {
            name: "openai".into(),
            model: Some("m".into()),
            api_key: Some("fixture-key".into()),
            base_url: Some(base),
        };
        let result = call_provider(
            &config,
            "system",
            &[json!({"role":"user","content":"script"})],
            &json!({}),
        )
        .await;
        assert_eq!(result.unwrap(), "{}");
        server.await.unwrap();
    }
}
