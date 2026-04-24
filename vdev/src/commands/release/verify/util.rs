// Shared helpers for release-verification probes.

use std::{collections::HashMap, io::Read, time::Duration};

use anyhow::{Context as _, Result};
use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};

const USER_AGENT: &str = "vdev-release-verify";

/// Blocking HTTP client with sane defaults for short requests (HEAD, small GETs).
pub fn client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent(USER_AGENT)
        .build()
        .map_err(Into::into)
}

/// Blocking HTTP client tuned for large streaming downloads (e.g. SHA256 sampling
/// of multi-MB artifacts).
pub fn stream_client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(300))
        .user_agent(USER_AGENT)
        .build()
        .map_err(Into::into)
}

/// HEAD `url` and return the advertised Content-Length (0 if the server omits it).
/// Follows redirects (reqwest's default policy is up to 10 hops).
pub fn head_size(client: &reqwest::blocking::Client, url: &str) -> Result<u64> {
    let resp = client
        .head(url)
        .send()
        .with_context(|| format!("HEAD {url}"))?
        .error_for_status()
        .with_context(|| format!("HEAD {url}"))?;
    Ok(content_length(&resp).unwrap_or(0))
}

/// GET `url` and return the body as a UTF-8 string. Bails on non-2xx.
pub fn fetch_text(client: &reqwest::blocking::Client, url: &str) -> Result<String> {
    client
        .get(url)
        .send()
        .with_context(|| format!("GET {url}"))?
        .error_for_status()
        .with_context(|| format!("GET {url}"))?
        .text()
        .with_context(|| format!("reading body of {url}"))
}

/// GET `url`, gunzip the body, and return the decompressed text.
pub fn fetch_gz_text(client: &reqwest::blocking::Client, url: &str) -> Result<String> {
    let body = client
        .get(url)
        .send()
        .with_context(|| format!("GET {url}"))?
        .error_for_status()
        .with_context(|| format!("GET {url}"))?
        .bytes()
        .with_context(|| format!("reading body of {url}"))?;
    let mut decoded = String::new();
    GzDecoder::new(body.as_ref())
        .read_to_string(&mut decoded)
        .with_context(|| format!("gunzipping {url}"))?;
    Ok(decoded)
}

/// Stream `url` through SHA-256 without buffering the whole body. Returns the
/// lowercase hex digest. Suitable for multi-MB release artifacts.
pub fn stream_sha256(client: &reqwest::blocking::Client, url: &str) -> Result<String> {
    struct Hasher(Sha256);
    impl std::io::Write for Hasher {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.update(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let mut resp = client
        .get(url)
        .send()
        .with_context(|| format!("GET {url}"))?
        .error_for_status()
        .with_context(|| format!("GET {url}"))?;
    let mut hasher = Hasher(Sha256::new());
    std::io::copy(&mut resp, &mut hasher).with_context(|| format!("streaming {url}"))?;
    Ok(hex::encode(hasher.0.finalize()))
}

/// Parse coreutils-style `sha256sum` output into a `{filename: hex_digest}` map.
///
/// Handles both text mode (`<hex>  <filename>`, two spaces) and binary mode
/// (`<hex> *<filename>`, one space + asterisk). Skips blank and comment lines.
pub fn parse_sha256sums(body: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for line in body.lines() {
        let line = line.trim_end_matches('\r').trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((digest, rest)) = line.split_once(char::is_whitespace) else {
            continue;
        };
        let name = rest.trim_start();
        let name = name.strip_prefix('*').unwrap_or(name).trim();
        out.insert(name.to_string(), digest.to_string());
    }
    out
}

fn content_length(resp: &reqwest::blocking::Response) -> Option<u64> {
    resp.headers()
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
}

#[cfg(test)]
mod tests {
    use super::parse_sha256sums;

    #[test]
    fn parses_coreutils_style() {
        let body = "\
abc123  vector-amd64.deb
def456 *vector.tar.gz

# comment
789ghi  vector.rpm
";
        let map = parse_sha256sums(body);
        assert_eq!(map.get("vector-amd64.deb").map(String::as_str), Some("abc123"));
        assert_eq!(map.get("vector.tar.gz").map(String::as_str), Some("def456"));
        assert_eq!(map.get("vector.rpm").map(String::as_str), Some("789ghi"));
    }
}
