//! AWS MSK IAM authentication for the `kafka` source and sink (SASL/OAUTHBEARER).
//!
//! librdkafka has no native `AWS_MSK_IAM` SASL mechanism (that lives only in the JVM client). The
//! portable path is SASL/OAUTHBEARER, where the OAuth token is a SigV4-presigned URL for the
//! `kafka-cluster:Connect` action, base64url-encoded. This is a port of the official
//! `aws-msk-iam-sasl-signer-go` `constructAuthToken`; correctness is proven offline in the tests
//! below against an independent SigV4 oracle (fixed inputs -> known-answer signature).

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use aws_credential_types::Credentials;
use aws_credential_types::provider::{ProvideCredentials, SharedCredentialsProvider};
use aws_sigv4::http_request::{
    SignableBody, SignableRequest, SignatureLocation, SigningSettings, sign,
};
use aws_sigv4::sign::v4;
use aws_smithy_runtime_api::client::identity::Identity;
use aws_types::region::Region;
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use snafu::{ResultExt, Snafu};

const EXPIRY_SECONDS: u64 = 900;
const SIGNING_NAME: &str = "kafka-cluster";
const ACTION: &str = "kafka-cluster:Connect";
const USER_AGENT: &str = concat!("vector-msk-iam-signer/", env!("CARGO_PKG_VERSION"));

type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

#[derive(Debug, Snafu)]
pub enum MskIamError {
    #[snafu(display("failed to load AWS credentials for MSK IAM auth: {source}"))]
    Credentials {
        source: aws_credential_types::provider::error::CredentialsError,
    },
    #[snafu(display("failed to build SigV4 signing params: {source}"))]
    SigningParams { source: BoxError },
    #[snafu(display("failed to presign MSK IAM token: {source}"))]
    Signing { source: BoxError },
}

/// Token librdkafka hands to the broker, plus its absolute expiry (unix-epoch ms).
#[derive(Debug, Clone)]
pub struct MskAuthToken {
    pub token: String,
    pub lifetime_ms: i64,
}

/// Generate a fresh MSK IAM SASL/OAUTHBEARER token for `region`.
pub async fn generate_auth_token(
    region: &Region,
    credentials_provider: &SharedCredentialsProvider,
) -> Result<MskAuthToken, MskIamError> {
    let credentials = credentials_provider
        .provide_credentials()
        .await
        .context(CredentialsSnafu)?;
    let signing_time = SystemTime::now();
    build_token(region, &credentials, signing_time)
}

/// Deterministic core: given credentials + a fixed signing time, produce the token. Split out so
/// tests can pin the time and assert a known-answer signature.
fn build_token(
    region: &Region,
    credentials: &Credentials,
    signing_time: SystemTime,
) -> Result<MskAuthToken, MskIamError> {
    let signed_url = presign(region, credentials, signing_time)?;
    // `User-Agent` is appended *after* signing (unsigned), matching the AWS signers.
    let signed_url = format!("{signed_url}&User-Agent={USER_AGENT}");
    let token = URL_SAFE_NO_PAD.encode(signed_url.as_bytes());

    let signing_ms = signing_time
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    Ok(MskAuthToken {
        token,
        lifetime_ms: signing_ms + (EXPIRY_SECONDS as i64) * 1000,
    })
}

/// Build the SigV4-presigned `https://kafka.{region}.amazonaws.com/?Action=...` URL.
fn presign(
    region: &Region,
    credentials: &Credentials,
    time: SystemTime,
) -> Result<String, MskIamError> {
    let host = format!("kafka.{}.amazonaws.com", region.as_ref());
    let identity: Identity = credentials.clone().into();

    let mut settings = SigningSettings::default();
    settings.signature_location = SignatureLocation::QueryParams;
    settings.expires_in = Some(Duration::from_secs(EXPIRY_SECONDS));

    let signing_params = v4::SigningParams::builder()
        .identity(&identity)
        .region(region.as_ref())
        .name(SIGNING_NAME)
        .time(time)
        .settings(settings)
        .build()
        .map_err(|e| MskIamError::SigningParams { source: Box::new(e) })?;

    let url = format!("https://{host}/?Action={}", rfc3986(ACTION));
    let headers = [("host", host.as_str())];
    let signable = SignableRequest::new("GET", &url, headers.into_iter(), SignableBody::Bytes(&[]))
        .map_err(|e| MskIamError::Signing { source: Box::new(e) })?;

    let output = sign(signable, &signing_params.into())
        .map_err(|e| MskIamError::Signing { source: Box::new(e) })?;
    let (instructions, _signature) = output.into_parts();

    // `params()` yields DECODED (name, value) pairs. They must be re-encoded with the exact RFC3986
    // rules AWS used when signing the canonical request, or the emitted URL won't match its own
    // signature and the broker will reject the token (e.g. `/` in X-Amz-Credential -> `%2F`).
    let mut final_url = url.clone();
    for (name, value) in instructions.params() {
        final_url.push('&');
        final_url.push_str(&rfc3986(name));
        final_url.push('=');
        final_url.push_str(&rfc3986(value));
    }
    Ok(final_url)
}

/// RFC3986 percent-encoding matching AWS SigV4 canonicalization (unreserved = A-Za-z0-9-_.~).
fn rfc3986(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // Fixed inputs shared with oracle/sigv4_oracle.py. 1_700_000_000 == 2023-11-14T22:13:20Z.
    const ACCESS_KEY: &str = "AKIDEXAMPLE";
    const SECRET_KEY: &str = "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY";
    const SESSION_TOKEN: &str = "IQoJb3Jp";
    const FIXED_EPOCH_SECS: u64 = 1_700_000_000;

    // Golden signatures produced by the independent Python oracle.
    const GOLDEN_SIG_WITH_TOKEN: &str =
        "4c02acd7700d674cecd2b27f6a906be056df725db5c7571700fd54a354a31904";
    const GOLDEN_SIG_NO_TOKEN: &str =
        "12f697bba1fe524b7e9f370e2c56075ea612d9d4d2c7f16c402ffc72520fce5e";

    fn region() -> Region {
        Region::new("ap-southeast-1")
    }

    fn fixed_time() -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(FIXED_EPOCH_SECS)
    }

    fn query_param<'a>(url: &'a str, key: &str) -> Option<&'a str> {
        let q = url.split_once('?')?.1;
        q.split('&').find_map(|kv| {
            let (k, v) = kv.split_once('=')?;
            (k == key).then_some(v)
        })
    }

    /// Split a URL into (base, sorted [(key,value)]) so two URLs can be compared for identical
    /// content regardless of query-parameter ordering (MSK re-canonicalizes, so order is irrelevant).
    fn normalize(url: &str) -> (String, Vec<(String, String)>) {
        let (base, query) = url.split_once('?').expect("url has query");
        let mut params: Vec<(String, String)> = query
            .split('&')
            .map(|kv| {
                let (k, v) = kv.split_once('=').expect("kv");
                (k.to_string(), v.to_string())
            })
            .collect();
        params.sort();
        (base.to_string(), params)
    }

    // Full presigned URLs from the independent oracle (oracle/sigv4_oracle.py output).
    const ORACLE_URL_WITH_TOKEN: &str = "https://kafka.ap-southeast-1.amazonaws.com/?Action=kafka-cluster%3AConnect&X-Amz-Algorithm=AWS4-HMAC-SHA256&X-Amz-Credential=AKIDEXAMPLE%2F20231114%2Fap-southeast-1%2Fkafka-cluster%2Faws4_request&X-Amz-Date=20231114T221320Z&X-Amz-Expires=900&X-Amz-Security-Token=IQoJb3Jp&X-Amz-SignedHeaders=host&X-Amz-Signature=4c02acd7700d674cecd2b27f6a906be056df725db5c7571700fd54a354a31904";

    /// STRONGEST PROOF: the *entire* presigned URL (every param, every encoded value, the signature)
    /// matches the independent oracle — not just the signature. Order-independent because MSK
    /// re-canonicalizes the query on its side.
    #[test]
    fn full_presigned_url_matches_oracle() {
        let creds =
            Credentials::from_keys(ACCESS_KEY, SECRET_KEY, Some(SESSION_TOKEN.to_string()));
        let url = presign(&region(), &creds, fixed_time()).expect("presign");
        assert_eq!(normalize(&url), normalize(ORACLE_URL_WITH_TOKEN), "url={url}");
    }

    /// KNOWN-ANSWER: the aws-sigv4 signature must equal the independent oracle's, byte-for-byte,
    /// with a session token present (X-Amz-Security-Token in the signed canonical query).
    #[test]
    fn signature_matches_oracle_with_session_token() {
        let creds =
            Credentials::from_keys(ACCESS_KEY, SECRET_KEY, Some(SESSION_TOKEN.to_string()));
        let url = presign(&region(), &creds, fixed_time()).expect("presign");

        assert_eq!(
            query_param(&url, "X-Amz-Signature"),
            Some(GOLDEN_SIG_WITH_TOKEN),
            "aws-sigv4 signature diverged from the independent SigV4 oracle\nurl={url}"
        );
        assert_eq!(query_param(&url, "X-Amz-Security-Token"), Some("IQoJb3Jp"));
    }

    /// KNOWN-ANSWER: same, without a session token (static long-term credentials).
    #[test]
    fn signature_matches_oracle_without_session_token() {
        let creds = Credentials::from_keys(ACCESS_KEY, SECRET_KEY, None);
        let url = presign(&region(), &creds, fixed_time()).expect("presign");

        assert_eq!(
            query_param(&url, "X-Amz-Signature"),
            Some(GOLDEN_SIG_NO_TOKEN),
            "aws-sigv4 signature diverged from the independent SigV4 oracle\nurl={url}"
        );
        assert_eq!(query_param(&url, "X-Amz-Security-Token"), None);
    }

    /// Structural invariants the MSK broker requires of the presigned URL.
    #[test]
    fn presigned_url_has_required_msk_shape() {
        let creds = Credentials::from_keys(ACCESS_KEY, SECRET_KEY, None);
        let url = presign(&region(), &creds, fixed_time()).expect("presign");

        assert!(url.starts_with("https://kafka.ap-southeast-1.amazonaws.com/?"));
        assert_eq!(query_param(&url, "Action"), Some("kafka-cluster%3AConnect"));
        assert_eq!(query_param(&url, "X-Amz-Algorithm"), Some("AWS4-HMAC-SHA256"));
        assert_eq!(query_param(&url, "X-Amz-Expires"), Some("900"));
        assert_eq!(query_param(&url, "X-Amz-SignedHeaders"), Some("host"));
        assert_eq!(
            query_param(&url, "X-Amz-Credential"),
            Some("AKIDEXAMPLE%2F20231114%2Fap-southeast-1%2Fkafka-cluster%2Faws4_request")
        );
    }

    /// The final token is base64url-nopad of the presigned URL (+ appended User-Agent).
    #[test]
    fn token_is_base64url_nopad_of_presigned_url() {
        let creds = Credentials::from_keys(ACCESS_KEY, SECRET_KEY, None);
        let out = build_token(&region(), &creds, fixed_time()).expect("token");

        assert!(!out.token.contains('='), "token must be unpadded base64url");
        let decoded = URL_SAFE_NO_PAD.decode(out.token.as_bytes()).expect("decode");
        let url = String::from_utf8(decoded).expect("utf8");
        assert!(url.starts_with("https://kafka.ap-southeast-1.amazonaws.com/?"));
        assert!(url.ends_with(&format!("&User-Agent={USER_AGENT}")));
        assert!(url.contains(GOLDEN_SIG_NO_TOKEN));

        // Absolute expiry = signing time + 900s, in unix-epoch ms.
        assert_eq!(out.lifetime_ms, (FIXED_EPOCH_SECS as i64) * 1000 + 900_000);
    }

    /// End-to-end through the public async entrypoint with a static credentials provider.
    #[tokio::test]
    async fn generate_auth_token_via_provider() {
        let provider = SharedCredentialsProvider::new(Credentials::from_keys(
            ACCESS_KEY,
            SECRET_KEY,
            Some(SESSION_TOKEN.to_string()),
        ));
        let out = generate_auth_token(&region(), &provider)
            .await
            .expect("generate");

        let decoded = URL_SAFE_NO_PAD.decode(out.token.as_bytes()).expect("decode");
        let url = String::from_utf8(decoded).expect("utf8");
        assert!(url.contains("Action=kafka-cluster%3AConnect"));
        assert!(url.contains("X-Amz-Signature="));
        // Uses wall-clock now(): lifetime must be ~900s in the future, so comfortably positive.
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        assert!(out.lifetime_ms > now_ms);
        assert!(out.lifetime_ms <= now_ms + 900_000 + 5_000);
    }
}
