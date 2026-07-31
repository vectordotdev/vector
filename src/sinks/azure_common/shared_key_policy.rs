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
/// This policy injects the required x-ms-date header and adds the
/// `Authorization: SharedKey {account}:{signature}` header. The x-ms-version header is supplied
/// by the Azure SDK and signed without being changed. The signature is computed according to the
/// "Authorize with Shared Key" rules for the Blob service:
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
}

impl SharedKeyAuthorizationPolicy {
    /// Create a new shared key policy.
    ///
    /// - `account_name`: The storage account name.
    /// - `account_key_b64`: Base64-encoded storage account key.
    pub fn new(account_name: String, account_key_b64: String) -> AzureResult<Self> {
        let account_key = base64::decode(account_key_b64.as_bytes()).map_err(|e| {
            AzureError::with_message(
                azure_core::error::ErrorKind::Other,
                format!("invalid account key base64: {e}"),
            )
        })?;
        Ok(Self {
            account_name,
            account_key,
        })
    }

    fn ensure_ms_date_header(&self, request: &mut Request) {
        let now = OffsetDateTime::now_utc();
        let ms_date = to_rfc7231(&now);
        request.insert_header("x-ms-date", ms_date);
    }

    fn build_string_to_sign(&self, req: &Request) -> AzureResult<String> {
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

        for required_header in ["x-ms-date", "x-ms-version"] {
            if header(required_header).is_none() {
                return Err(AzureError::with_message(
                    azure_core::error::ErrorKind::Other,
                    format!("missing required {required_header} header"),
                ));
            }
        }

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

        // Content-Length (include value if present; keep "0")
        if let Some(v) = header("Content-Length") {
            s.push_str(v);
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
        // The Azure SDK sets x-ms-version before running policies. Add x-ms-date, then sign the
        // exact headers that will be sent.
        self.ensure_ms_date_header(request);
        // Build string to sign
        let sts = self.build_string_to_sign(request)?;
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
        SharedKeyAuthorizationPolicy::new("account".into(), "a2V5".into()).unwrap()
    }

    #[test]
    fn build_string_to_sign_includes_existing_service_version() {
        let policy = policy();
        let mut request = Request::new(
            Url::parse("https://account.blob.core.windows.net/container/blob").unwrap(),
            Method::Put,
        );
        request.insert_header("x-ms-version", "2021-08-06");

        policy.ensure_ms_date_header(&mut request);
        let string_to_sign = policy.build_string_to_sign(&request).unwrap();

        let service_version = request
            .headers()
            .iter()
            .find(|(name, _)| name.as_str().eq_ignore_ascii_case("x-ms-version"))
            .map(|(_, value)| value.as_str());
        assert_eq!(service_version, Some("2021-08-06"));
        assert!(string_to_sign.contains("x-ms-version:2021-08-06\n"));
    }

    #[test]
    fn build_string_to_sign_rejects_missing_service_version() {
        let policy = policy();
        let mut request = Request::new(
            Url::parse("https://account.blob.core.windows.net/container/blob").unwrap(),
            Method::Put,
        );
        policy.ensure_ms_date_header(&mut request);

        let error = policy.build_string_to_sign(&request).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("missing required x-ms-version header")
        );
    }
}
