//! models.dev catalog: the model list the setup wizard offers per provider.
//!
//! Cached at `<home>/cache/models.dev.json` for 24 hours; a corrupt cache is
//! refetched, and a fetch failure silently falls back to any parseable cached
//! copy, however old. A fetched body is held in memory until [`ModelCatalog::publish`]
//! so a cancelled wizard leaves no cache behind.
use std::{
    fmt,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use serde_json::Value;

use crate::{atomic_file, fetch::MAX_BODY_BYTES};

pub const MODELS_DEV_URL: &str = "https://models.dev/api.json";
const CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);
const FETCH_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelListError {
    pub url: String,
}

impl fmt::Display for ModelListError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "could not load model list from {}. Check your connection and try again.",
            self.url
        )
    }
}

impl std::error::Error for ModelListError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelChoice {
    pub id: String,
    pub recommended: bool,
}

#[derive(Clone)]
pub struct ModelCatalog {
    data: Value,
    /// Freshly fetched bytes not yet written to the cache file.
    fetched: Option<Vec<u8>>,
}

impl ModelCatalog {
    pub async fn load(home: &Path, url: &str) -> Result<Self, ModelListError> {
        Self::load_at(home, url, SystemTime::now()).await
    }

    /// `now` is the clock used to judge cache freshness against the file's mtime.
    /// A cache file counts as fresh only if it parses and is younger than the TTL.
    pub async fn load_at(home: &Path, url: &str, now: SystemTime) -> Result<Self, ModelListError> {
        let path = cache_path(home);
        let cached: Option<Value> = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok());
        let fresh = cached.is_some()
            && std::fs::metadata(&path)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|mtime| now.duration_since(mtime).ok())
                .is_some_and(|age| age < CACHE_TTL);
        if !fresh
            && let Ok(body) = fetch(url).await
            && let Ok(data) = serde_json::from_slice(&body)
        {
            return Ok(Self {
                data,
                fetched: Some(body),
            });
        }
        cached
            .map(Self::from_value)
            .ok_or_else(|| ModelListError { url: url.into() })
    }

    pub fn from_value(data: Value) -> Self {
        Self {
            data,
            fetched: None,
        }
    }

    /// Writes a fetched body to the cache file; a no-op for a catalog served from cache.
    pub fn publish(&self, home: &Path) -> std::io::Result<()> {
        match &self.fetched {
            Some(body) => atomic_file::write_atomically(&cache_path(home), body, 0o644),
            None => Ok(()),
        }
    }

    /// Usable models for `provider_key`, newest first; the first `recommended`
    /// regex match is flagged and moved to the top.
    pub fn models(&self, provider_key: &str, recommended: Option<&str>) -> Vec<ModelChoice> {
        let Some(models) = self.data[provider_key]["models"].as_object() else {
            return Vec::new();
        };
        let mut usable: Vec<(&str, Option<&str>)> = models
            .iter()
            .filter(|(_, m)| is_usable(m))
            .map(|(id, m)| (id.as_str(), m["release_date"].as_str()))
            .collect();
        usable.sort_by(|a, b| {
            let by_date = match (a.1, b.1) {
                (Some(x), Some(y)) => y.cmp(x),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            };
            by_date.then_with(|| a.0.cmp(b.0))
        });
        let mut choices: Vec<ModelChoice> = usable
            .into_iter()
            .map(|(id, _)| ModelChoice {
                id: id.into(),
                recommended: false,
            })
            .collect();
        if let Some(re) = recommended.and_then(|p| regex::Regex::new(p).ok())
            && let Some(pos) = choices.iter().position(|c| re.is_match(&c.id))
        {
            let mut pick = choices.remove(pos);
            pick.recommended = true;
            choices.insert(0, pick);
        }
        choices
    }
}

fn is_usable(model: &Value) -> bool {
    let has_text = |direction: &str| {
        model["modalities"][direction]
            .as_array()
            .is_some_and(|list| list.iter().any(|v| v == "text"))
    };
    model["tool_call"] == Value::Bool(true)
        && model["status"].as_str() != Some("deprecated")
        && has_text("input")
        && has_text("output")
}

/// Bodies over [`MAX_BODY_BYTES`] are refused, by Content-Length or while streaming.
async fn fetch(url: &str) -> Result<Vec<u8>, FetchError> {
    let client = reqwest::Client::builder().timeout(FETCH_TIMEOUT).build()?;
    let mut response = client.get(url).send().await?.error_for_status()?;
    if response
        .content_length()
        .is_some_and(|n| n > MAX_BODY_BYTES as u64)
    {
        return Err(FetchError::TooLarge);
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if body.len() + chunk.len() > MAX_BODY_BYTES {
            return Err(FetchError::TooLarge);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

/// Both outcomes fall back to the cache; the cause is not reported.
enum FetchError {
    Network,
    TooLarge,
}
impl From<reqwest::Error> for FetchError {
    fn from(_: reqwest::Error) -> Self {
        Self::Network
    }
}

fn cache_path(home: &Path) -> PathBuf {
    home.join("cache").join("models.dev.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::{Read, Write};

    fn model(
        tool_call: Value,
        status: Option<&str>,
        date: Option<&str>,
        modalities: Value,
    ) -> Value {
        let mut m = json!({ "tool_call": tool_call, "modalities": modalities });
        if let Some(s) = status {
            m["status"] = json!(s);
        }
        if let Some(d) = date {
            m["release_date"] = json!(d);
        }
        m
    }

    fn text() -> Value {
        json!({ "input": ["text", "image"], "output": ["text"] })
    }

    fn ids(choices: &[ModelChoice]) -> Vec<&str> {
        choices.iter().map(|c| c.id.as_str()).collect()
    }

    fn catalog(models: Value) -> ModelCatalog {
        ModelCatalog::from_value(json!({ "anthropic": { "models": models } }))
    }

    #[test]
    fn keeps_only_text_tool_calling_non_deprecated_models() {
        let cat = catalog(json!({
            "keep": model(json!(true), Some("active"), Some("2025-01-01"), text()),
            "no-tools": model(json!(false), None, Some("2025-01-02"), text()),
            "tools-missing": { "modalities": text(), "release_date": "2025-01-03" },
            "tools-string": model(json!("true"), None, Some("2025-01-04"), text()),
            "old": model(json!(true), Some("deprecated"), Some("2025-01-05"), text()),
            "image-out": model(json!(true), None, Some("2025-01-06"),
                json!({ "input": ["text"], "output": ["image"] })),
            "audio-in": model(json!(true), None, Some("2025-01-07"),
                json!({ "input": ["audio"], "output": ["text"] })),
            "no-modalities": { "tool_call": true, "release_date": "2025-01-08" },
        }));
        assert_eq!(ids(&cat.models("anthropic", None)), ["keep"]);
        assert!(cat.models("openai", None).is_empty());
    }

    #[test]
    fn sorts_newest_first_with_undated_last_and_uses_key_over_id_field() {
        let cat = catalog(json!({
            "mid": model(json!(true), None, Some("2024-06-01"), text()),
            "undated": model(json!(true), None, None, text()),
            "new": { "id": "other-id", "tool_call": true, "release_date": "2025-03-01", "modalities": text() },
            "old": model(json!(true), None, Some("2023-01-01"), text()),
        }));
        assert_eq!(
            ids(&cat.models("anthropic", None)),
            ["new", "mid", "old", "undated"]
        );
    }

    #[test]
    fn same_release_date_breaks_ties_by_id() {
        let cat = catalog(json!({
            "b-model": model(json!(true), None, Some("2025-01-01"), text()),
            "a-model": model(json!(true), None, Some("2025-01-01"), text()),
            "z-undated": model(json!(true), None, None, text()),
            "c-model": model(json!(true), None, Some("2025-01-01"), text()),
            "a-undated": model(json!(true), None, None, text()),
        }));
        assert_eq!(
            ids(&cat.models("anthropic", None)),
            ["a-model", "b-model", "c-model", "a-undated", "z-undated"]
        );
    }

    #[test]
    fn first_regex_match_is_recommended_and_moved_to_top() {
        let cat = catalog(json!({
            "opus-2": model(json!(true), None, Some("2025-02-01"), text()),
            "sonnet-2": model(json!(true), None, Some("2025-01-15"), text()),
            "haiku-1": model(json!(true), None, Some("2025-01-01"), text()),
            "sonnet-1": model(json!(true), None, Some("2024-01-01"), text()),
        }));
        let choices = cat.models("anthropic", Some("^sonnet"));
        assert_eq!(ids(&choices), ["sonnet-2", "opus-2", "haiku-1", "sonnet-1"]);
        assert_eq!(
            choices.iter().map(|c| c.recommended).collect::<Vec<_>>(),
            [true, false, false, false]
        );
        let none = cat.models("anthropic", Some("gpt"));
        assert_eq!(ids(&none), ["opus-2", "sonnet-2", "haiku-1", "sonnet-1"]);
        assert!(none.iter().all(|c| !c.recommended));
    }

    fn fixture(providers: &[&str]) -> Value {
        let mut root = serde_json::Map::new();
        for p in providers {
            root.insert(
                (*p).into(),
                json!({ "models": { format!("{p}-model"): model(json!(true), None, None, text()) } }),
            );
        }
        Value::Object(root)
    }

    fn write_cache(home: &Path, data: &Value) -> SystemTime {
        let path = cache_path(home);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, serde_json::to_vec(data).unwrap()).unwrap();
        std::fs::metadata(&path).unwrap().modified().unwrap()
    }

    /// Serves `response` once and reports whether a request arrived.
    fn http_fixture(response: Vec<u8>) -> (String, std::sync::mpsc::Receiver<()>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let _ = tx.send(());
            let mut request = [0; 2048];
            let _ = socket.read(&mut request);
            let _ = socket.write_all(&response);
        });
        (url, rx)
    }

    fn ok_response(body: &Value) -> Vec<u8> {
        let body = serde_json::to_vec(body).unwrap();
        let mut response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .into_bytes();
        response.extend(body);
        response
    }

    #[tokio::test]
    async fn fresh_cache_is_used_without_network() {
        let home = tempfile::tempdir().unwrap();
        let mtime = write_cache(home.path(), &fixture(&["cached"]));
        let (url, hit) = http_fixture(ok_response(&fixture(&["remote"])));
        let cat = ModelCatalog::load_at(home.path(), &url, mtime + Duration::from_secs(23 * 3600))
            .await
            .unwrap();
        assert_eq!(ids(&cat.models("cached", None)), ["cached-model"]);
        assert!(
            hit.try_recv().is_err(),
            "fresh cache must not hit the network"
        );
    }

    #[tokio::test]
    async fn stale_cache_survives_fetch_failure() {
        let home = tempfile::tempdir().unwrap();
        let mtime = write_cache(home.path(), &fixture(&["stale"]));
        let (bad_json, _) = http_fixture(
            b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\n{{{".to_vec(),
        );
        let later = mtime + Duration::from_secs(25 * 3600);
        for url in ["http://127.0.0.1:1/".to_string(), bad_json] {
            let cat = ModelCatalog::load_at(home.path(), &url, later)
                .await
                .unwrap();
            assert_eq!(ids(&cat.models("stale", None)), ["stale-model"]);
        }
        let cached: Value =
            serde_json::from_slice(&std::fs::read(cache_path(home.path())).unwrap()).unwrap();
        assert_eq!(
            cached,
            fixture(&["stale"]),
            "failed fetch must not clobber cache"
        );
    }

    #[tokio::test]
    async fn stale_cache_is_refreshed_from_network() {
        let home = tempfile::tempdir().unwrap();
        let mtime = write_cache(home.path(), &fixture(&["stale"]));
        let (url, hit) = http_fixture(ok_response(&fixture(&["remote"])));
        let cat = ModelCatalog::load_at(home.path(), &url, mtime + Duration::from_secs(25 * 3600))
            .await
            .unwrap();
        assert!(hit.try_recv().is_ok());
        assert_eq!(ids(&cat.models("remote", None)), ["remote-model"]);
        assert!(cat.models("stale", None).is_empty());
        let read = || -> Value {
            serde_json::from_slice(&std::fs::read(cache_path(home.path())).unwrap()).unwrap()
        };
        assert_eq!(read(), fixture(&["stale"]), "fetch alone must not write");
        cat.publish(home.path()).unwrap();
        assert_eq!(read(), fixture(&["remote"]));
    }

    #[tokio::test]
    async fn corrupt_fresh_cache_is_refetched_and_overwritten() {
        let home = tempfile::tempdir().unwrap();
        let path = cache_path(home.path());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"{{{ not json").unwrap();
        let mtime = std::fs::metadata(&path).unwrap().modified().unwrap();
        let (url, hit) = http_fixture(ok_response(&fixture(&["remote"])));
        let cat = ModelCatalog::load_at(home.path(), &url, mtime + Duration::from_secs(60))
            .await
            .expect("corrupt cache must not block the network");
        assert!(hit.try_recv().is_ok(), "corrupt cache must be refetched");
        assert_eq!(ids(&cat.models("remote", None)), ["remote-model"]);
        cat.publish(home.path()).unwrap();
        let cached: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(cached, fixture(&["remote"]));
    }

    #[tokio::test]
    async fn oversized_body_is_refused_and_a_stale_cache_still_serves() {
        let home = tempfile::tempdir().unwrap();
        let huge = || {
            http_fixture(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    MAX_BODY_BYTES + 1
                )
                .into_bytes(),
            )
            .0
        };
        assert!(
            ModelCatalog::load_at(home.path(), &huge(), SystemTime::now())
                .await
                .is_err()
        );
        assert!(!cache_path(home.path()).exists());
        let mtime = write_cache(home.path(), &fixture(&["stale"]));
        let cat =
            ModelCatalog::load_at(home.path(), &huge(), mtime + Duration::from_secs(25 * 3600))
                .await
                .unwrap();
        assert_eq!(ids(&cat.models("stale", None)), ["stale-model"]);
    }

    #[tokio::test]
    async fn missing_cache_is_published_and_failure_without_cache_errors() {
        let home = tempfile::tempdir().unwrap();
        let err = ModelCatalog::load_at(home.path(), "http://127.0.0.1:1/", SystemTime::now())
            .await
            .err()
            .expect("no cache and unreachable URL must fail");
        assert_eq!(
            err.to_string(),
            "could not load model list from http://127.0.0.1:1/. Check your connection and try again."
        );
        assert!(!cache_path(home.path()).exists());

        let (url, _) = http_fixture(ok_response(&fixture(&["remote"])));
        let cat = ModelCatalog::load_at(home.path(), &url, SystemTime::now())
            .await
            .unwrap();
        assert_eq!(ids(&cat.models("remote", None)), ["remote-model"]);
        assert!(!home.path().join("cache").exists(), "unpublished");
        cat.publish(home.path()).unwrap();
        assert!(cache_path(home.path()).exists());
        ModelCatalog::from_value(fixture(&["x"]))
            .publish(home.path())
            .unwrap();
        let cached: Value =
            serde_json::from_slice(&std::fs::read(cache_path(home.path())).unwrap()).unwrap();
        assert_eq!(
            cached,
            fixture(&["remote"]),
            "cache-served catalog publishes nothing"
        );
    }
}
