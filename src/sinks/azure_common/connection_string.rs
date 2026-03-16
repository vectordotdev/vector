/*!
Minimal Azure Storage connection string parser and URL builder for Blob Storage.

This module intentionally avoids relying on the legacy Azure Storage SDK crates.
It extracts only the fields we need and composes container/blob URLs suitable
for the newer `azure_storage_blob` crate (>= 0.7).

Supported keys (case-insensitive):
- AccountName
- AccountKey
- SharedAccessSignature
- DefaultEndpointsProtocol
- EndpointSuffix
- BlobEndpoint
- UseDevelopmentStorage
- DevelopmentStorageProxyUri

Behavior
- If `BlobEndpoint` is present, it is used as the base for container/blob URLs.
  It may already include the account segment (e.g., Azurite: http://127.0.0.1:10000/devstoreaccount1).
- Otherwise, if `UseDevelopmentStorage=true`, we synthesize a dev endpoint:
  `{protocol}://127.0.0.1:10000/{account_name}`, with `protocol` default `http` if unspecified.
  If `DevelopmentStorageProxyUri` is present, it replaces the host/port while still appending
  the account name path segment.
- Otherwise, we synthesize the public cloud endpoint:
  `{protocol}://{account_name}.blob.{endpoint_suffix}` where `endpoint_suffix` defaults to `core.windows.net`
  and `protocol` defaults to `https`.

SAS handling
- If `SharedAccessSignature` exists, it will be appended to the generated URLs as a query string.
  Both `sv=...` and `?sv=...` forms are accepted; the leading '?' is normalized.

Examples:
- Access key connection string:
  "DefaultEndpointsProtocol=https;AccountName=myacct;AccountKey=base64key==;EndpointSuffix=core.windows.net"
  Container URL: <https://myacct.blob.core.windows.net/logs>
  Blob URL: <https://myacct.blob.core.windows.net/logs/file.txt>

- SAS connection string:
  "BlobEndpoint=<https://myacct.blob.core.windows.net/>;SharedAccessSignature=sv=2022-11-02&ss=b&..."
  Container URL (with SAS): <https://myacct.blob.core.windows.net/logs?sv=2022-11-02&ss=b&...>
  Blob URL (with SAS): <https://myacct.blob.core.windows.net/logs/file.txt?sv=2022-11-02&ss=b&...>

- Azurite/dev storage:
  "UseDevelopmentStorage=true;DefaultEndpointsProtocol=http;AccountName=devstoreaccount1"
  Container URL: <http://127.0.0.1:10000/devstoreaccount1/logs>
*/

use std::collections::HashMap;

/// Errors that can occur while parsing a connection string or composing URLs.
#[derive(Debug, Clone)]
pub enum ConnectionStringError {
    InvalidFormat(&'static str),
    InvalidPair(String),
    MissingAccountName,
    MissingEndpoint,
}

impl std::fmt::Display for ConnectionStringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectionStringError::InvalidFormat(msg) => write!(f, "invalid format: {msg}"),
            ConnectionStringError::InvalidPair(p) => write!(f, "invalid key=value pair: {p}"),
            ConnectionStringError::MissingAccountName => write!(f, "account name is required"),
            ConnectionStringError::MissingEndpoint => {
                write!(f, "could not determine Blob endpoint")
            }
        }
    }
}

impl std::error::Error for ConnectionStringError {}

/// Represents the type of authentication present in the connection string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Auth {
    /// Shared key-based authentication (account key).
    SharedKey {
        account_name: String,
        account_key: String, // base64-encoded account key as provided
    },
    /// Shared access signature provided as query string (without the leading `?`).
    Sas { query: String },
    /// No credentials present.
    None,
}

/// A parsed Azure Storage connection string and helpers to compose URLs for containers/blobs.
#[derive(Debug, Clone, Default)]
pub struct ParsedConnectionString {
    pub account_name: Option<String>,
    pub account_key: Option<String>,
    pub shared_access_signature: Option<String>,
    pub default_endpoints_protocol: Option<String>,
    pub endpoint_suffix: Option<String>,
    pub blob_endpoint: Option<String>,
    pub use_development_storage: bool,
    pub development_storage_proxy_uri: Option<String>,
}

impl ParsedConnectionString {
    /// Parse a connection string into a `ParsedConnectionString`.
    ///
    /// The parser is case-insensitive for keys and ignores empty segments.
    pub fn parse(s: &str) -> Result<Self, ConnectionStringError> {
        let mut map: HashMap<String, String> = HashMap::new();

        for seg in s.split(';') {
            let seg = seg.trim();
            if seg.is_empty() {
                continue;
            }
            let (k, v) = seg
                .split_once('=')
                .ok_or_else(|| ConnectionStringError::InvalidPair(seg.to_string()))?;
            let key = k.trim().to_ascii_lowercase();
            let value = v.trim().to_string();
            map.insert(key, value);
        }

        // Build the structure from the parsed map.
        let parsed = ParsedConnectionString {
            account_name: map.get("accountname").cloned(),
            account_key: map.get("accountkey").cloned(),
            shared_access_signature: map
                .get("sharedaccesssignature")
                .map(|s| normalize_sas(s.as_str())),
            default_endpoints_protocol: map
                .get("defaultendpointsprotocol")
                .map(|s| s.to_ascii_lowercase()),
            endpoint_suffix: map.get("endpointsuffix").cloned(),
            blob_endpoint: map.get("blobendpoint").cloned(),
            use_development_storage: map
                .get("usedevelopmentstorage")
                .map(|v| v.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
            development_storage_proxy_uri: map.get("developmentstorageproxyuri").cloned(),
        };

        Ok(parsed)
    }

    /// Determine the authentication method present in this connection string.
    pub fn auth(&self) -> Auth {
        if let (Some(name), Some(key)) = (self.account_name.as_ref(), self.account_key.as_ref()) {
            return Auth::SharedKey {
                account_name: name.clone(),
                account_key: key.clone(),
            };
        }
        if let Some(sas) = self.shared_access_signature.as_ref() {
            return Auth::Sas { query: sas.clone() };
        }
        Auth::None
    }

    /// Get the normalized default protocol, defaulting to:
    /// - http for development storage
    /// - https otherwise
    pub fn default_protocol(&self) -> String {
        if let Some(p) = self.default_endpoints_protocol.as_deref() {
            match p {
                "http" | "https" => p.to_string(),
                _ => {
                    // Fallbacks
                    if self.use_development_storage {
                        "http".to_string()
                    } else {
                        "https".to_string()
                    }
                }
            }
        } else if self.use_development_storage {
            "http".to_string()
        } else {
            "https".to_string()
        }
    }

    /// Get the normalized endpoint suffix, defaulting to "core.windows.net".
    pub fn endpoint_suffix(&self) -> String {
        self.endpoint_suffix
            .clone()
            .unwrap_or_else(|| "core.windows.net".to_string())
    }

    /// Build the base Blob endpoint URL (no container/blob path).
    ///
    /// Resolution order:
    /// 1. BlobEndpoint (as-is, without trailing slash normalization)
    /// 2. Development storage synthesized URL: `{proto}://127.0.0.1:10000/{account}`
    ///    If DevelopmentStorageProxyUri is present, it will be used instead of 127.0.0.1:10000.
    /// 3. Public cloud synthesized URL: `{proto}://{account}.blob.{suffix}`
    pub fn blob_account_endpoint(&self) -> Result<String, ConnectionStringError> {
        if let Some(explicit) = self.blob_endpoint.as_ref() {
            return Ok(explicit.clone());
        }

        let account_name = self
            .account_name
            .as_ref()
            .ok_or(ConnectionStringError::MissingAccountName)?;

        let proto = self.default_protocol();

        if self.use_development_storage {
            // If the proxy URI is provided, use it. Otherwise default to 127.0.0.1:10000
            let host = self
                .development_storage_proxy_uri
                .as_deref()
                .map(|s| s.trim_end_matches('/').to_string())
                .unwrap_or_else(|| "127.0.0.1:10000".to_string());

            let base = if host.starts_with("http://") || host.starts_with("https://") {
                format!("{}/{}", trim_trailing_slash(&host), account_name)
            } else {
                format!("{proto}://{host}/{}", account_name)
            };
            return Ok(base);
        }

        // Public cloud-style base
        let suffix = self.endpoint_suffix();
        Ok(format!("{proto}://{}.blob.{}", account_name, suffix))
    }

    /// Build a container URL, optionally appending SAS if present.
    pub fn container_url(&self, container: &str) -> Result<String, ConnectionStringError> {
        let base = self.blob_account_endpoint()?;
        Ok(append_query_segment(
            &format!("{}/{}", trim_trailing_slash(&base), container),
            self.shared_access_signature.as_deref(),
        ))
    }

    /// Build a blob URL, optionally appending SAS if present.
    pub fn blob_url(&self, container: &str, blob: &str) -> Result<String, ConnectionStringError> {
        // Build the base container URL without SAS, then append the blob path,
        // and finally append the SAS so it appears after the full path.
        let base = self.blob_account_endpoint()?;
        let container_no_sas = format!("{}/{}", trim_trailing_slash(&base), container);
        let blob_full = format!(
            "{}/{}",
            trim_trailing_slash(&container_no_sas),
            encode_path_segment(blob)
        );
        Ok(append_query_segment(
            &blob_full,
            self.shared_access_signature.as_deref(),
        ))
    }
}

/// Normalize a SAS string by removing any leading '?'.
fn normalize_sas(s: &str) -> String {
    s.trim_start_matches('?').to_string()
}

/// Append a query segment `sas` to `base_url`, respecting whether `base_url` already has a query.
fn append_query_segment(base_url: &str, sas: Option<&str>) -> String {
    match sas {
        None => base_url.to_string(),
        Some("") => base_url.to_string(),
        Some(q) => {
            let sep = if base_url.contains('?') { '&' } else { '?' };
            format!("{base_url}{sep}{q}")
        }
    }
}

/// Trim exactly one trailing slash from a string, if present.
fn trim_trailing_slash(s: &str) -> String {
    if let Some(stripped) = s.strip_suffix('/') {
        stripped.to_string()
    } else {
        s.to_string()
    }
}

/// Encode a path segment minimally (only slash needs special handling for our cases).
/// For our purposes (blob names generated by Vector), we only replace spaces with %20.
/// This avoids pulling an extra crate; refine if needed in the future.
fn encode_path_segment(seg: &str) -> String {
    seg.replace(' ', "%20")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_access_key_public_cloud() {
        let cs = "DefaultEndpointsProtocol=https;AccountName=myacct;AccountKey=base64==;EndpointSuffix=core.windows.net";
        let parsed = ParsedConnectionString::parse(cs).unwrap();
        assert_eq!(parsed.account_name.as_deref(), Some("myacct"));
        assert_eq!(parsed.account_key.as_deref(), Some("base64=="));
        assert!(parsed.shared_access_signature.is_none());
        assert_eq!(parsed.default_protocol(), "https");
        assert_eq!(parsed.endpoint_suffix(), "core.windows.net");

        let base = parsed.blob_account_endpoint().unwrap();
        assert_eq!(base, "https://myacct.blob.core.windows.net");

        let container_url = parsed.container_url("logs").unwrap();
        assert_eq!(container_url, "https://myacct.blob.core.windows.net/logs");

        let blob_url = parsed.blob_url("logs", "file.txt").unwrap();
        assert_eq!(
            blob_url,
            "https://myacct.blob.core.windows.net/logs/file.txt"
        );
        assert_eq!(
            parsed.auth(),
            Auth::SharedKey {
                account_name: "myacct".to_string(),
                account_key: "base64==".to_string()
            }
        );
    }

    #[test]
    fn parse_sas_with_blob_endpoint() {
        let cs = "BlobEndpoint=https://myacct.blob.core.windows.net/;SharedAccessSignature=sv=2022-11-02&ss=b&srt=sco&sp=rcw&se=2099-01-01T00:00:00Z&sig=...";
        let parsed = ParsedConnectionString::parse(cs).unwrap();
        assert_eq!(
            parsed.shared_access_signature.as_deref(),
            Some("sv=2022-11-02&ss=b&srt=sco&sp=rcw&se=2099-01-01T00:00:00Z&sig=...")
        );

        let container_url = parsed.container_url("logs").unwrap();
        assert_eq!(
            container_url,
            "https://myacct.blob.core.windows.net/logs?sv=2022-11-02&ss=b&srt=sco&sp=rcw&se=2099-01-01T00:00:00Z&sig=..."
        );

        let blob_url = parsed.blob_url("logs", "file name.txt").unwrap();
        assert_eq!(
            blob_url,
            "https://myacct.blob.core.windows.net/logs/file%20name.txt?sv=2022-11-02&ss=b&srt=sco&sp=rcw&se=2099-01-01T00:00:00Z&sig=..."
        );
        assert_eq!(
            parsed.auth(),
            Auth::Sas {
                query: "sv=2022-11-02&ss=b&srt=sco&sp=rcw&se=2099-01-01T00:00:00Z&sig=..."
                    .to_string()
            }
        );
    }

    #[test]
    fn parse_sas_with_leading_question_mark() {
        let cs = "BlobEndpoint=https://myacct.blob.core.windows.net/;SharedAccessSignature=?sv=2022-11-02&ss=b";
        let parsed = ParsedConnectionString::parse(cs).unwrap();
        assert_eq!(
            parsed.shared_access_signature.as_deref(),
            Some("sv=2022-11-02&ss=b")
        );
        let url = parsed.container_url("logs").unwrap();
        assert_eq!(
            url,
            "https://myacct.blob.core.windows.net/logs?sv=2022-11-02&ss=b"
        );
    }

    #[test]
    fn parse_development_storage_with_defaults() {
        let cs =
            "UseDevelopmentStorage=true;DefaultEndpointsProtocol=http;AccountName=devstoreaccount1";
        let parsed = ParsedConnectionString::parse(cs).unwrap();
        let base = parsed.blob_account_endpoint().unwrap();
        assert_eq!(base, "http://127.0.0.1:10000/devstoreaccount1");

        let container_url = parsed.container_url("logs").unwrap();
        assert_eq!(
            container_url,
            "http://127.0.0.1:10000/devstoreaccount1/logs"
        );
    }

    #[test]
    fn parse_development_storage_with_proxy() {
        let cs = "UseDevelopmentStorage=true;AccountName=devstoreaccount1;DevelopmentStorageProxyUri=http://localhost:10000";
        let parsed = ParsedConnectionString::parse(cs).unwrap();
        let base = parsed.blob_account_endpoint().unwrap();
        assert_eq!(base, "http://localhost:10000/devstoreaccount1");

        let container_url = parsed.container_url("logs").unwrap();
        assert_eq!(
            container_url,
            "http://localhost:10000/devstoreaccount1/logs"
        );
    }

    #[test]
    fn parse_invalid_pairs() {
        let cs = "AccountName;AccountKey=noequals";
        let err = ParsedConnectionString::parse(cs).unwrap_err();
        match err {
            ConnectionStringError::InvalidPair(p) => {
                assert!(p == "AccountName" || p == "AccountKey=noequals")
            }
            _ => panic!("unexpected error: {err}"),
        }
    }
}
