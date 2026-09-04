use std::{collections::BTreeMap, fmt::Write as _, sync::Arc};

use async_trait::async_trait;
use azure_core::http::policies::{Policy, PolicyResult};
use azure_core::http::{Context, Request, Url};
use azure_core::{
    Result as AzureResult, base64,
    error::Error as AzureError,
    time::{OffsetDateTime, to_rfc7231},
};

use openssl::{hash::MessageDigest, pkey::PKey, sign::Signer};

/// Shared Key authorization policy for Azure Blob Storage requests.
///
/// This policy injects the required headers (x-ms-date, x-ms-version, and
/// content-length) if missing and adds the `Authorization: SharedKey {account}:{signature}` header. The signature
/// is computed according to the "Authorize with Shared Key" rules for the Blob service:
///
/// StringToSign =
///   VERB + "\n" +
///   Content-Encoding + "\n" +
///   Content-Language + "\n" +
///   Content-Length + "\n" +
///   Content-MD5 + "\n" +
///   Content-Type + "\n" +
///   Date + "\n" +
///   If-Modified-Since + "\n" +
///   If-Match + "\n" +
///   If-None-Match + "\n" +
///   If-Unmodified-Since + "\n" +
///   Range + "\n" +
///   CanonicalizedHeaders +
///   CanonicalizedResource
///
/// Notes:
/// - We set x-ms-date, leaving the standard Date field empty in the signature.
/// - If Content-Length header is present with "0", the canonicalized value must be the empty string.
/// - Canonicalized headers include all x-ms-* headers (lowercased, sorted).
/// - Canonicalized resource is "/{account}{path}\n" + sorted lowercase query params.
///
#[derive(Debug)]
pub struct SharedKeyAuthorizationPolicy {
    account_name: String,
    account_key: Vec<u8>, // decoded from base64
    storage_version: String,
}

impl SharedKeyAuthorizationPolicy {
    /// Create a new shared key policy.
    ///
    /// - `account_name`: The storage account name.
    /// - `account_key_b64`: Base64-encoded storage account key.
    /// - `storage_version`: x-ms-version value to send (e.g. "2025-11-05").
    pub fn new(
        account_name: String,
        account_key_b64: String,
        storage_version: String,
    ) -> AzureResult<Self> {
        let account_key = base64::decode(account_key_b64.as_bytes()).map_err(|e| {
            AzureError::with_message(
                azure_core::error::ErrorKind::Other,
                format!("invalid account key base64: {e}"),
            )
        })?;
        Ok(Self {
            account_name,
            account_key,
            storage_version,
        })
    }

    fn ensure_signing_headers(&self, request: &mut Request) -> AzureResult<(String, String)> {
        // Always set x-ms-date and x-ms-version explicitly to known values for signing.
        let now = OffsetDateTime::now_utc();
        let ms_date = to_rfc7231(&now);
        request.insert_header("x-ms-date", ms_date.clone());
        let ms_version = self.storage_version.clone();
        request.insert_header("x-ms-version", ms_version.clone());

        // Set a known body length before signing so the signature and wire request use the
        // same explicit value. Preserve a Content-Length supplied by the SDK.
        let has_content_length = request
            .headers()
            .iter()
            .any(|(name, _)| name.as_str().eq_ignore_ascii_case("content-length"));
        if !has_content_length && let Some(content_length) = request.body().len() {
            request.insert_header("content-length", content_length.to_string());
        }

        Ok((ms_date, ms_version))
    }

    fn build_string_to_sign(
        &self,
        req: &Request,
        ms_date: &str,
        ms_version: &str,
    ) -> AzureResult<String> {
        let method = req.method().as_str();
        let url = req.url();

        let mut s = String::with_capacity(512);

        // VERB
        s.push_str(method);
        s.push('\n');

        // Resolve standard headers (case-insensitive) and write them in order required by the spec.
        // https://learn.microsoft.com/en-us/rest/api/storageservices/authorize-with-shared-key#shared-key-format-for-2009-09-19-and-later
        let header = |name: &str| -> Option<&str> {
            for (n, v) in req.headers().iter() {
                if n.as_str().eq_ignore_ascii_case(name) {
                    return Some(v.as_str());
                }
            }
            None
        };

        // Content-Encoding
        if let Some(v) = header("Content-Encoding") {
            s.push_str(v);
        }
        s.push('\n');

        // Content-Language
        if let Some(v) = header("Content-Language") {
            s.push_str(v);
        }
        s.push('\n');

        // Content-Length
        // Azure's Shared Key format represents zero length as an empty field.
        let content_length = header("Content-Length").filter(|value| *value != "0");
        if let Some(content_length) = content_length {
            s.push_str(content_length);
        }
        s.push('\n');

        // Content-MD5
        if let Some(v) = header("Content-MD5") {
            s.push_str(v);
        }
        s.push('\n');

        // Content-Type
        if let Some(v) = header("Content-Type") {
            s.push_str(v);
        }
        s.push('\n');

        // Date (unused when x-ms-date is used)
        s.push('\n');

        // If-Modified-Since
        if let Some(v) = header("If-Modified-Since") {
            s.push_str(v);
        }
        s.push('\n');

        // If-Match
        if let Some(v) = header("If-Match") {
            s.push_str(v);
        }
        s.push('\n');

        // If-None-Match
        if let Some(v) = header("If-None-Match") {
            s.push_str(v);
        }
        s.push('\n');

        // If-Unmodified-Since
        if let Some(v) = header("If-Unmodified-Since") {
            s.push_str(v);
        }
        s.push('\n');

        // Range
        if let Some(v) = header("Range") {
            s.push_str(v);
        }
        s.push('\n');

        // CanonicalizedHeaders: include all x-ms-* headers, lowercased, sorted by name.
        // If multiple values for the same header exist, sort values and join with commas.
        let mut xms: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (name, value) in req.headers().iter() {
            let key = name.as_str().to_ascii_lowercase();
            if key.starts_with("x-ms-") {
                xms.entry(key)
                    .or_default()
                    .push(value.as_str().trim().to_string());
            }
        }
        // Ensure required headers are present (they should have been inserted).
        xms.entry("x-ms-date".to_string())
            .or_default()
            .push(ms_date.to_string());
        xms.entry("x-ms-version".to_string())
            .or_default()
            .push(ms_version.to_string());

        for (k, mut vals) in xms {
            vals.sort();
            vals.dedup();
            let joined = vals.join(",");
            writeln!(s, "{}:{}", k, joined).ok();
        }

        // CanonicalizedResource
        append_canonicalized_resource(&mut s, &self.account_name, url)?;

        Ok(s)
    }

    fn sign(&self, string_to_sign: &str) -> AzureResult<String> {
        let pkey = PKey::hmac(&self.account_key).map_err(|e| {
            AzureError::with_message(
                azure_core::error::ErrorKind::Other,
                format!("failed to create HMAC key: {e}"),
            )
        })?;
        let mut signer = Signer::new(MessageDigest::sha256(), &pkey).map_err(|e| {
            AzureError::with_message(
                azure_core::error::ErrorKind::Other,
                format!("failed to create signer: {e}"),
            )
        })?;
        signer.update(string_to_sign.as_bytes()).map_err(|e| {
            AzureError::with_message(
                azure_core::error::ErrorKind::Other,
                format!("signer update failed: {e}"),
            )
        })?;
        let mac = signer.sign_to_vec().map_err(|e| {
            AzureError::with_message(
                azure_core::error::ErrorKind::Other,
                format!("signer sign failed: {e}"),
            )
        })?;
        Ok(base64::encode(&mac))
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl Policy for SharedKeyAuthorizationPolicy {
    async fn send(
        &self,
        ctx: &Context,
        request: &mut Request,
        next: &[Arc<dyn Policy>],
    ) -> PolicyResult {
        // Ensure required signing headers are present
        let (ms_date, ms_version) = self.ensure_signing_headers(request)?;
        // Build string to sign
        let sts = self.build_string_to_sign(request, &ms_date, &ms_version)?;
        let signature = self.sign(&sts)?;

        // Authorization: SharedKey {account}:{signature}
        request.insert_header(
            "authorization",
            format!("SharedKey {}:{}", self.account_name, signature),
        );

        // Continue pipeline
        next[0].send(ctx, request, &next[1..]).await
    }
}

// ---------- Helpers ----------

fn append_canonicalized_resource(s: &mut String, account: &str, url: &Url) -> AzureResult<()> {
    // "/{account_name}{path}\n"
    s.push('/');
    s.push_str(account);
    // Append the URL path exactly as-is (per spec).
    s.push_str(url.path());

    // Canonicalized query: lowercase names, sort by name, join multi-values by comma, each line "name:value\n"
    // https://learn.microsoft.com/en-us/rest/api/storageservices/authorize-with-shared-key#shared-key-format-for-2009-09-19-and-later
    if url.query().is_some() {
        let mut qp_map: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (name, value) in url.query_pairs() {
            let key_l = name.to_ascii_lowercase();
            let v = value.to_string();
            if v.is_empty() {
                continue;
            }
            qp_map.entry(key_l).or_default().push(v);
        }
        for (k, mut vals) in qp_map {
            vals.sort();
            let mut line = String::new();
            write!(&mut line, "\n{}:", k).ok();
            let joined = vals.join(",");
            line.push_str(&joined);
            s.push_str(&line);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use azure_core::http::Method;

    use super::*;

    fn policy() -> SharedKeyAuthorizationPolicy {
        SharedKeyAuthorizationPolicy::new(
            "account".to_owned(),
            "ZmFrZS10ZXN0LWFjY291bnQta2V5".to_owned(),
            "2025-11-05".to_owned(),
        )
        .expect("test key should be valid base64")
    }

    fn content_length_header(request: &Request) -> Option<&str> {
        request.headers().iter().find_map(|(name, value)| {
            name.as_str()
                .eq_ignore_ascii_case("content-length")
                .then_some(value.as_str())
        })
    }

    fn content_length_field(request: &mut Request) -> String {
        let policy = policy();
        policy
            .ensure_signing_headers(request)
            .expect("signing headers should be added");
        policy
            .build_string_to_sign(request, "Thu, 30 Jul 2026 16:02:25 GMT", "2025-11-05")
            .expect("request should be signed")
            .lines()
            .nth(3)
            .expect("string to sign should contain content length")
            .to_owned()
    }

    #[test]
    fn sets_and_signs_the_body_length_when_content_length_is_missing() {
        let mut request = Request::new(
            Url::parse("https://account.blob.core.windows.net/container/blob?comp=blocklist")
                .expect("test URL should be valid"),
            Method::Put,
        );
        request.set_body(vec![0_u8; 123]);

        assert_eq!(content_length_field(&mut request), "123");
        assert_eq!(content_length_header(&request), Some("123"));
    }

    #[test]
    fn preserves_a_nonzero_content_length_header() {
        let mut request = Request::new(
            Url::parse("https://account.blob.core.windows.net/container/blob")
                .expect("test URL should be valid"),
            Method::Put,
        );
        request.insert_header("content-length", "42");
        request.set_body(vec![0_u8; 123]);

        assert_eq!(content_length_field(&mut request), "42");
        assert_eq!(content_length_header(&request), Some("42"));
    }

    #[test]
    fn canonicalizes_zero_content_length_as_empty() {
        let mut request = Request::new(
            Url::parse("https://account.blob.core.windows.net/container/blob")
                .expect("test URL should be valid"),
            Method::Put,
        );
        request.insert_header("content-length", "0");

        assert_eq!(content_length_field(&mut request), "");
        assert_eq!(content_length_header(&request), Some("0"));
    }
}
