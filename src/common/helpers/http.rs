use std::net::IpAddr;
use std::sync::OnceLock;
use std::time::Duration;

use crate::common::error::{AppError, AppResult};

const MAX_BYTES: usize = 2 * 1024 * 1024;
const MAX_REDIRECTS: usize = 5;

// shared client with a timeout: fetch_bytes and the NBRB rates sit on the
// upload request path, a hung connection would stall upload indefinitely.
// Auto-redirects are off on purpose: fetch_bytes follows them manually so
// every hop re-passes the private-address check.
pub fn client() -> AppResult<&'static reqwest::Client> {
    static CLIENT: OnceLock<Result<reqwest::Client, String>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .map_err(|e| e.to_string())
        })
        .as_ref()
        .map_err(|e| AppError::Internal(format!("cannot build http client: {e}")))
}

fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_documentation()
        }
        IpAddr::V6(v6) => {
            let s = v6.segments();
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (s[0] & 0xfe00) == 0xfc00 // unique local fc00::/7
                || (s[0] & 0xffc0) == 0xfe80 // link local fe80::/10
        }
    }
}

async fn ensure_public_host(url: &reqwest::Url) -> AppResult<()> {
    // host_str keeps '[]' around IPv6 literals; brackets can't appear in domains
    let Some(host) = url.host_str().map(|h| h.trim_matches(|c| c == '[' || c == ']')) else {
        return Err(AppError::BadRequest(format!("url '{url}' has no host")));
    };
    let addrs: Vec<IpAddr> = match host.parse::<IpAddr>() {
        Ok(ip) => vec![ip],
        Err(_) => {
            let port = url.port_or_known_default().unwrap_or(80);
            tokio::net::lookup_host((host, port))
                .await
                .map_err(|e| {
                    AppError::ExternalApi(format!("cannot resolve host '{host}': {e}"))
                })?
                .map(|a| a.ip())
                .collect()
        }
    };
    if addrs.is_empty() {
        return Err(AppError::ExternalApi(format!("host '{host}' resolves to no address")));
    }
    if addrs.iter().any(|a| is_private_ip(*a)) {
        return Err(AppError::BadRequest(format!(
            "url host '{host}' points to a private address"
        )));
    }
    Ok(())
}

fn check_scheme(url: &reqwest::Url) -> AppResult<()> {
    if !matches!(url.scheme(), "http" | "https") {
        return Err(AppError::BadRequest(format!(
            "unsupported url scheme '{}', expected http or https",
            url.scheme()
        )));
    }
    Ok(())
}

pub async fn fetch_bytes(url: &str) -> AppResult<Vec<u8>> {
    let mut url = reqwest::Url::parse(url.trim())
        .map_err(|e| AppError::BadRequest(format!("invalid url '{url}': {e}")))?;
    check_scheme(&url)?;

    // literal IPs and localhost resolve without touching the network, so the
    // private-address guard runs before any connection is attempted
    let mut response = None;
    for _ in 0..=MAX_REDIRECTS {
        ensure_public_host(&url).await?;
        let current = client()?
            .get(url.clone())
            .header("user-agent", "expense-tracker")
            .send()
            .await
            .map_err(|e| AppError::ExternalApi(format!("cannot fetch '{url}': {e}")))?;
        if !current.status().is_redirection() {
            response = Some(
                current
                    .error_for_status()
                    .map_err(|e| AppError::ExternalApi(format!("cannot fetch '{url}': {e}")))?,
            );
            break;
        }
        let location = current
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| {
                AppError::ExternalApi(format!("redirect from '{url}' has no location header"))
            })?;
        url = url.join(location).map_err(|e| {
            AppError::ExternalApi(format!("invalid redirect '{location}' from '{url}': {e}"))
        })?;
        check_scheme(&url)?;
    }
    let Some(mut response) = response else {
        return Err(AppError::ExternalApi(format!(
            "too many redirects (limit {MAX_REDIRECTS})"
        )));
    };

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

    #[tokio::test]
    async fn private_hosts_are_bad_request() {
        for url in [
            "http://127.0.0.1:3000/api/receipts/pending",
            "http://0.0.0.0/x",
            "http://[::1]:3000/x",
            "http://[fe80::1]/x",
            "http://[fc00::1]/x",
            "https://localhost/x",
            "http://10.0.0.5/r.html",
            "http://192.168.1.10/r.html",
            "http://169.254.169.254/latest/meta-data/",
        ] {
            let err = fetch_bytes(url).await.unwrap_err();
            assert!(
                matches!(err, AppError::BadRequest(ref msg) if msg.contains("private")),
                "url: {url} -> {err:?}"
            );
        }
    }

    #[test]
    fn test_is_private_ip() {
        let private = [
            "127.0.0.1", "10.0.0.1", "192.168.1.1", "172.16.0.1", "169.254.169.254", "0.0.0.0",
            "::1", "fe80::1", "fc00::1", "fd12::1",
        ];
        for ip in private {
            assert!(is_private_ip(ip.parse().unwrap()), "ip: {ip}");
        }
        let public = ["93.84.109.201", "8.8.8.8", "2a02:6b8::2", "2001:db8:1::1"];
        for ip in public {
            assert!(!is_private_ip(ip.parse().unwrap()), "ip: {ip}");
        }
    }
}
