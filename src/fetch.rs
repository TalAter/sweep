use std::time::Duration;
use tokio_util::sync::CancellationToken;
#[derive(Debug, Clone)]
pub struct FetchedScript {
    pub bytes: Vec<u8>,
    pub sha256: String,
    pub final_url: String,
    pub fetched_at: String,
    pub status: u16,
}
#[derive(Debug, Clone, thiserror::Error)]
#[error("{message}")]
pub struct FetchScriptError {
    pub reason: String,
    pub message: String,
}
pub async fn fetch_script(
    url: &str,
    cancel: &CancellationToken,
) -> Result<FetchedScript, FetchScriptError> {
    fetch_script_with_timeout(url, cancel, Duration::from_secs(30)).await
}
pub async fn fetch_script_with_timeout(
    url: &str,
    cancel: &CancellationToken,
    timeout: Duration,
) -> Result<FetchedScript, FetchScriptError> {
    use futures_util::StreamExt;
    use sha2::{Digest, Sha256};
    const MAX: usize = 5 * 1024 * 1024;
    let err = |reason: &str, message: String| FetchScriptError {
        reason: reason.into(),
        message,
    };
    let request = async {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::limited(20))
            .build()
            .map_err(|e| err("network", format!("network error fetching {url}: {e}")))?;
        let response = client
            .get(url)
            .send()
            .await
            .map_err(|e| err("network", format!("network error fetching {url}: {e}")))?;
        let fetched_at = crate::store::now();
        let status = response.status();
        if !status.is_success() {
            return Err(err("non-2xx", format!("HTTP {status}: {url}")));
        }
        if let Some(size) = response.content_length().filter(|n| *n > MAX as u64) {
            return Err(err(
                "too-large",
                format!(
                    "script body {size} bytes exceeds {MAX}-byte limit (per Content-Length): {url}"
                ),
            ));
        }
        let final_url = response.url().to_string();
        let mut stream = response.bytes_stream();
        let mut bytes = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| {
                err(
                    "network",
                    format!("network error reading body from {url}: {e}"),
                )
            })?;
            if bytes.len() + chunk.len() > MAX {
                return Err(err(
                    "too-large",
                    format!("script body exceeds {MAX}-byte limit: {url}"),
                ));
            }
            bytes.extend_from_slice(&chunk);
        }
        let sha256 = Sha256::digest(&bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        Ok(FetchedScript {
            bytes,
            sha256,
            final_url,
            fetched_at,
            status: status.as_u16(),
        })
    };
    tokio::select! {biased;_=cancel.cancelled()=>Err(err("timeout",format!("request timed out after {}ms: {url}",timeout.as_millis()))), result=tokio::time::timeout(timeout,request)=>result.unwrap_or_else(|_|Err(err("timeout",format!("request timed out after {}ms: {url}",timeout.as_millis()))))}
}
