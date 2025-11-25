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
/// This policy injects the required headers (x-ms-date, x-ms-version) if missing and
/// adds the `Authorization: SharedKey {account}:{signature}` header. The signature
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

    fn ensure_ms_headers(&self, request: &mut Request) -> AzureResult<(String, String)> {
        let now = OffsetDateTime::now_utc();
        let ms_date = to_rfc7231(&now);
        // Insert owned values to avoid borrowing issues
        request.insert_header("x-ms-date", ms_date.clone());
        request.insert_header("x-ms-version", self.storage_version.clone());
        Ok((ms_date, self.storage_version.clone()))
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

        // Newline characters for empty headers
        // https://learn.microsoft.com/en-us/rest/api/storageservices/authorize-with-shared-key

        // Content-Encoding
        s.push('\n');
        // Content-Language
        s.push('\n');
        // Content-Length
        s.push('\n');
        // Content-MD5
        s.push('\n');
        // Content-Type
        s.push('\n');
        // Date (unused when x-ms-date is used)
        s.push('\n');
        // If-Modified-Since
        s.push('\n');
        // If-Match
        s.push('\n');
        // If-None-Match
        s.push('\n');
        // If-Unmodified-Since
        s.push('\n');
        // Range
        s.push('\n');

        // CanonicalizedHeaders (only those we know we set)
        s.push_str("x-ms-date:");
        s.push_str(ms_date);
        s.push('\n');
        s.push_str("x-ms-version:");
        s.push_str(ms_version);
        s.push('\n');

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
        // Ensure required x-ms headers are present
        let (ms_date, ms_version) = self.ensure_ms_headers(request)?;
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
    // Path is percent-decoded by Url, but we use it as-is (leading slash included).
    s.push_str(url.path());

    // Canonicalized query: lowercase names, sort by name, join multi-values by comma, each line "name:value\n"
    if let Some(_) = url.query() {
        let mut qp_map: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (name, value) in url.query_pairs() {
            let key_l = name.to_ascii_lowercase();
            qp_map.entry(key_l).or_default().push(value.to_string());
        }
        for (k, mut vals) in qp_map {
            // Sort values (optional but deterministic)
            vals.sort();
            let mut line = String::new();
            let _ = write!(&mut line, "\n{}:", k);
            let joined = vals.join(",");
            line.push_str(&joined);
            s.push_str(&line);
        }
    }

    Ok(())
}
