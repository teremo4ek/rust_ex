use std::sync::OnceLock;
use std::time::Duration;

use crate::common::error::{AppError, AppResult};

const MAX_BYTES: usize = 2 * 1024 * 1024;

// shared client with a timeout: fetch_bytes sits on the upload request path,
// a hung connection would stall upload indefinitely
fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_default()
    })
}

pub async fn fetch_bytes(url: &str) -> AppResult<Vec<u8>> {
    let url = reqwest::Url::parse(url.trim())
        .map_err(|e| AppError::BadRequest(format!("invalid url '{url}': {e}")))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(AppError::BadRequest(format!(
            "unsupported url scheme '{}', expected http or https",
            url.scheme()
        )));
    }

    // some receipt hosts reject requests without a user agent
    let response = client()
        .get(url.clone())
        .header("user-agent", "expense-tracker")
        .send()
        .await
        .map_err(|e| AppError::ExternalApi(format!("cannot fetch '{url}': {e}")))?;
    let mut response = response
        .error_for_status()
        .map_err(|e| AppError::ExternalApi(format!("cannot fetch '{url}': {e}")))?;

    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| AppError::ExternalApi(format!("cannot read '{url}': {e}")))?
    {
        if body.len() + chunk.len() > MAX_BYTES {
            return Err(AppError::BadRequest(format!(
                "url content exceeds {MAX_BYTES} bytes"
            )));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn invalid_url_is_bad_request() {
        let err = fetch_bytes("not a url").await.unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
    }

    #[tokio::test]
    async fn non_http_scheme_is_bad_request() {
        for url in ["ftp://example.com/r.html", "file:///etc/passwd"] {
            let err = fetch_bytes(url).await.unwrap_err();
            assert!(matches!(err, AppError::BadRequest(_)), "url: {url}");
        }
    }
}
