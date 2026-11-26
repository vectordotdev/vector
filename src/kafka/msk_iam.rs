//! AWS MSK IAM authentication for the `kafka` source and sink (SASL/OAUTHBEARER).
//!
//! librdkafka has no native `AWS_MSK_IAM` SASL mechanism (that lives only in the JVM client). The
//! portable path is SASL/OAUTHBEARER, where the OAuth token is a SigV4-presigned URL for the
//! `kafka-cluster:Connect` action, base64url-encoded. This is a port of the official
//! `aws-msk-iam-sasl-signer-go` `constructAuthToken`; correctness is proven offline in the tests
//! below against an independent `SigV4` oracle (fixed inputs -> known-answer signature).
//!
//! Token minting is fully synchronous: `build_token` only signs (no I/O, no async). AWS credential
//! resolution — the one async step — is done ahead of time and cached in an `Arc<Mutex<Credentials>>`
//! that a background task refreshes before expiry. This keeps `generate_oauth_token` free of any
//! `block_on`, which is essential because for the source it is invoked while the `StreamConsumer` is
//! polled *inside* the tokio runtime, where nested blocking would panic.

use std::fmt::Write as _;
use std::sync::{Arc, Mutex, Weak};
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
use vector_lib::internal_event::emit;

use crate::internal_events::KafkaMskIamCredentialsRefreshError;

/// MSK IAM presigned tokens are valid for at most 15 minutes.
const EXPIRY_SECONDS: u64 = 900;
/// Lifetime advertised to librdkafka, kept well below the 15-minute presign. librdkafka refreshes
/// the token at 80% of the advertised lifetime; advertising the full window would leave the current
/// token only ~3 minutes of validity at that point, which MSK can reject during SASL
/// re-authentication (`connections.max.reauth.ms`) as "Session too short". With 9 minutes the
/// refresh happens around minute 7, while the token is still valid for ~8 more minutes.
const MAX_ADVERTISED_LIFETIME: Duration = Duration::from_mins(9);
const SIGNING_NAME: &str = "kafka-cluster";
const ACTION: &str = "kafka-cluster:Connect";
const USER_AGENT: &str = concat!("vector-msk-iam-signer/", env!("CARGO_PKG_VERSION"));

/// Refresh cached credentials this long before they expire.
const CRED_REFRESH_SKEW: Duration = Duration::from_mins(5);
/// Back-off after a failed or no-progress refresh, so a provider that keeps failing (or keeps
/// returning credentials that are already inside the refresh window) doesn't spin.
const CRED_REFRESH_MIN: Duration = Duration::from_mins(1);
/// Cadence used when credentials advertise no expiry (e.g. static keys).
const CRED_REFRESH_NO_EXPIRY: Duration = Duration::from_mins(15);

type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

#[derive(Debug, Snafu)]
pub(crate) enum MskIamError {
    #[snafu(display("failed to load AWS credentials for MSK IAM auth: {source}"))]
    Credentials {
        source: aws_credential_types::provider::error::CredentialsError,
    },
    #[snafu(display("timed out after {timeout:?} loading AWS credentials for MSK IAM auth"))]
    CredentialsTimeout { timeout: Duration },
    #[snafu(display("failed to build SigV4 signing params: {source}"))]
    SigningParams { source: BoxError },
    #[snafu(display("failed to presign MSK IAM token: {source}"))]
    Signing { source: BoxError },
}

/// Shared, background-refreshed AWS credentials used to mint MSK IAM tokens.
pub(crate) type CredentialsCache = Arc<Mutex<Credentials>>;

/// Token librdkafka hands to the broker, plus its absolute expiry (unix-epoch ms).
#[derive(Debug, Clone)]
pub(crate) struct MskAuthToken {
    pub(crate) token: String,
    pub(crate) lifetime_ms: i64,
}

/// Resolve credentials once (async), then spawn a background task that keeps them fresh, and return
/// the shared cache. The `generate_oauth_token` callback reads this cache synchronously.
///
/// The refresher holds only a `Weak` reference, so it exits automatically once the owning
/// component (and thus the last `CredentialsCache` clone) is dropped.
pub(crate) async fn spawn_credentials_cache(
    provider: SharedCredentialsProvider,
    load_timeout: Duration,
) -> Result<CredentialsCache, MskIamError> {
    let initial = load_credentials(&provider, load_timeout).await?;
    let cache: CredentialsCache = Arc::new(Mutex::new(initial));
    // Keep the component's span so credential-refresh HTTP telemetry stays attributed to it.
    crate::spawn_in_current_span(refresh_loop(provider, load_timeout, Arc::downgrade(&cache)));
    Ok(cache)
}

/// Resolve credentials, honoring the configured load timeout so a stalled IMDS/STS/profile lookup
/// can't hang the component build (matches `AwsAuthentication::credentials_cache` semantics).
async fn load_credentials(
    provider: &SharedCredentialsProvider,
    load_timeout: Duration,
) -> Result<Credentials, MskIamError> {
    match tokio::time::timeout(load_timeout, provider.provide_credentials()).await {
        Ok(res) => res.context(CredentialsSnafu),
        Err(_) => Err(MskIamError::CredentialsTimeout {
            timeout: load_timeout,
        }),
    }
}

async fn refresh_loop(
    provider: SharedCredentialsProvider,
    load_timeout: Duration,
    weak: Weak<Mutex<Credentials>>,
) {
    // Credentials inside the refresh window are reloaded right away; only a failed or no-progress
    // attempt backs off, so cached credentials don't expire while waiting out the floor.
    let mut backoff = false;
    loop {
        let sleep_for = match weak.upgrade() {
            None => break,
            Some(cache) => next_refresh(cache.lock().ok().and_then(|c| c.expiry()), backoff),
        };
        tokio::time::sleep(sleep_for).await;

        let Some(cache) = weak.upgrade() else { break };
        // On failure/timeout report it, keep the previous credentials and retry after the back-off.
        backoff = match load_credentials(&provider, load_timeout).await {
            Ok(fresh) => {
                // Still inside the refresh window (e.g. a provider that keeps handing out the same
                // near-expiry credentials until it rotates them): no progress, so back off.
                let no_progress = next_refresh(fresh.expiry(), false).is_zero();
                if let Ok(mut guard) = cache.lock() {
                    *guard = fresh;
                }
                no_progress
            }
            Err(error) => {
                emit(KafkaMskIamCredentialsRefreshError {
                    error: error.into(),
                });
                true
            }
        };
    }
}

fn next_refresh(expiry: Option<SystemTime>, backoff: bool) -> Duration {
    let until_refresh = match expiry {
        Some(exp) => exp
            .duration_since(SystemTime::now())
            .unwrap_or(Duration::ZERO)
            .saturating_sub(CRED_REFRESH_SKEW),
        None => CRED_REFRESH_NO_EXPIRY,
    };
    if backoff {
        until_refresh.max(CRED_REFRESH_MIN)
    } else {
        until_refresh
    }
}

/// The MSK IAM signing host for a region, honoring an explicit `endpoint` override and otherwise
/// resolving the partition suffix (e.g. `amazonaws.com.cn` for China regions).
pub(crate) fn signing_host(region: &str, endpoint: Option<&str>) -> String {
    if let Some(endpoint) = endpoint {
        // Reduce to the bare authority (`host[:port]`): strip the scheme, then drop any
        // `/path`, `?query`, or `#fragment` so it is valid as both the URL host and Host header.
        let no_scheme = endpoint
            .trim_start_matches("https://")
            .trim_start_matches("http://");
        let authority = no_scheme.split(['/', '?', '#']).next().unwrap_or(no_scheme);
        return authority.to_string();
    }
    let suffix = if region.starts_with("cn-") {
        "amazonaws.com.cn"
    } else {
        "amazonaws.com"
    };
    format!("kafka.{region}.{suffix}")
}

/// Mint a fresh MSK IAM SASL/OAUTHBEARER token for `region`/`host` from already-resolved
/// `credentials`. Fully synchronous — safe to call from librdkafka's refresh callback on any thread.
pub(crate) fn build_token(
    region: &Region,
    host: &str,
    credentials: &Credentials,
    signing_time: SystemTime,
) -> Result<MskAuthToken, MskIamError> {
    let signed_url = presign(region, host, credentials, signing_time)?;
    // `User-Agent` is appended *after* signing (unsigned), matching the AWS signers.
    let signed_url = format!("{signed_url}&User-Agent={USER_AGENT}");
    let token = URL_SAFE_NO_PAD.encode(signed_url.as_bytes());

    // Advertise at most `MAX_ADVERTISED_LIFETIME` (see there), and never past the credentials that
    // signed the token: temporary (STS/IMDS) credentials may expire sooner, and librdkafka must
    // refresh before the underlying AWS session dies.
    let token_expiry = signing_time + MAX_ADVERTISED_LIFETIME;
    let effective_expiry = match credentials.expiry() {
        Some(cred_expiry) if cred_expiry < token_expiry => cred_expiry,
        _ => token_expiry,
    };
    let lifetime_ms = i64::try_from(
        effective_expiry
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
    )
    .unwrap_or(i64::MAX);
    Ok(MskAuthToken { token, lifetime_ms })
}

/// Build the SigV4-presigned `https://{host}/?Action=...` URL.
fn presign(
    region: &Region,
    host: &str,
    credentials: &Credentials,
    time: SystemTime,
) -> Result<String, MskIamError> {
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
        .map_err(|e| MskIamError::SigningParams {
            source: Box::new(e),
        })?;

    let url = format!("https://{host}/?Action={}", rfc3986(ACTION));
    let headers = [("host", host)];
    let signable = SignableRequest::new("GET", &url, headers.into_iter(), SignableBody::Bytes(&[]))
        .map_err(|e| MskIamError::Signing {
            source: Box::new(e),
        })?;

    let output = sign(signable, &signing_params.into()).map_err(|e| MskIamError::Signing {
        source: Box::new(e),
    })?;
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

/// RFC3986 percent-encoding matching AWS `SigV4` canonicalization (unreserved = A-Za-z0-9-_.~).
fn rfc3986(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => write!(out, "%{b:02X}").expect("writing to a String cannot fail"),
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

    // The oracle signed `kafka.ap-southeast-1.amazonaws.com` for this region.
    const HOST: &str = "kafka.ap-southeast-1.amazonaws.com";

    fn region() -> Region {
        Region::new("ap-southeast-1")
    }

    fn fixed_time() -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(FIXED_EPOCH_SECS)
    }

    fn creds(session: bool) -> Credentials {
        Credentials::from_keys(
            ACCESS_KEY,
            SECRET_KEY,
            session.then(|| SESSION_TOKEN.to_string()),
        )
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

    // Full presigned URL from the independent oracle (oracle/sigv4_oracle.py output).
    const ORACLE_URL_WITH_TOKEN: &str = "https://kafka.ap-southeast-1.amazonaws.com/?Action=kafka-cluster%3AConnect&X-Amz-Algorithm=AWS4-HMAC-SHA256&X-Amz-Credential=AKIDEXAMPLE%2F20231114%2Fap-southeast-1%2Fkafka-cluster%2Faws4_request&X-Amz-Date=20231114T221320Z&X-Amz-Expires=900&X-Amz-Security-Token=IQoJb3Jp&X-Amz-SignedHeaders=host&X-Amz-Signature=4c02acd7700d674cecd2b27f6a906be056df725db5c7571700fd54a354a31904";

    /// STRONGEST PROOF: the entire presigned URL (every param, every encoded value, the signature)
    /// matches the independent oracle — order-independent because MSK re-canonicalizes the query.
    #[test]
    fn full_presigned_url_matches_oracle() {
        let url = presign(&region(), HOST, &creds(true), fixed_time()).expect("presign");
        assert_eq!(
            normalize(&url),
            normalize(ORACLE_URL_WITH_TOKEN),
            "url={url}"
        );
    }

    /// KNOWN-ANSWER: aws-sigv4 signature must equal the oracle's, with a session token present.
    #[test]
    fn signature_matches_oracle_with_session_token() {
        let url = presign(&region(), HOST, &creds(true), fixed_time()).expect("presign");
        assert_eq!(
            query_param(&url, "X-Amz-Signature"),
            Some(GOLDEN_SIG_WITH_TOKEN)
        );
        assert_eq!(query_param(&url, "X-Amz-Security-Token"), Some("IQoJb3Jp"));
    }

    /// KNOWN-ANSWER: same, without a session token (static long-term credentials).
    #[test]
    fn signature_matches_oracle_without_session_token() {
        let url = presign(&region(), HOST, &creds(false), fixed_time()).expect("presign");
        assert_eq!(
            query_param(&url, "X-Amz-Signature"),
            Some(GOLDEN_SIG_NO_TOKEN)
        );
        assert_eq!(query_param(&url, "X-Amz-Security-Token"), None);
    }

    /// Structural invariants the MSK broker requires of the presigned URL.
    #[test]
    fn presigned_url_has_required_msk_shape() {
        let url = presign(&region(), HOST, &creds(false), fixed_time()).expect("presign");
        assert!(url.starts_with("https://kafka.ap-southeast-1.amazonaws.com/?"));
        assert_eq!(query_param(&url, "Action"), Some("kafka-cluster%3AConnect"));
        assert_eq!(
            query_param(&url, "X-Amz-Algorithm"),
            Some("AWS4-HMAC-SHA256")
        );
        assert_eq!(query_param(&url, "X-Amz-Expires"), Some("900"));
        assert_eq!(query_param(&url, "X-Amz-SignedHeaders"), Some("host"));
        assert_eq!(
            query_param(&url, "X-Amz-Credential"),
            Some("AKIDEXAMPLE%2F20231114%2Fap-southeast-1%2Fkafka-cluster%2Faws4_request")
        );
    }

    /// The token is base64url-nopad of the presigned URL (+ appended User-Agent), and its lifetime
    /// is the absolute unix-epoch ms of signing-time + the advertised 540s (not the 900s presign).
    #[test]
    fn build_token_encodes_and_sets_lifetime() {
        let out = build_token(&region(), HOST, &creds(false), fixed_time()).expect("token");
        assert!(!out.token.contains('='), "token must be unpadded base64url");
        let decoded = URL_SAFE_NO_PAD
            .decode(out.token.as_bytes())
            .expect("decode");
        let url = String::from_utf8(decoded).expect("utf8");
        assert!(url.starts_with("https://kafka.ap-southeast-1.amazonaws.com/?"));
        assert!(url.ends_with(&format!("&User-Agent={USER_AGENT}")));
        assert!(url.contains(GOLDEN_SIG_NO_TOKEN));
        assert_eq!(query_param(&url, "X-Amz-Expires"), Some("900"));
        assert_eq!(
            out.lifetime_ms,
            FIXED_EPOCH_SECS.cast_signed() * 1000 + 540_000
        );
    }

    #[test]
    fn signing_host_resolves_partition_and_endpoint() {
        // Commercial partition.
        assert_eq!(
            signing_host("us-east-1", None),
            "kafka.us-east-1.amazonaws.com"
        );
        // China partition uses the .com.cn suffix.
        assert_eq!(
            signing_host("cn-north-1", None),
            "kafka.cn-north-1.amazonaws.com.cn"
        );
        // Explicit endpoint overrides, with scheme/trailing slash stripped.
        assert_eq!(
            signing_host("us-east-1", Some("https://kafka.example.internal/")),
            "kafka.example.internal"
        );
        // A documented full-URL endpoint with a path reduces to just the authority.
        assert_eq!(
            signing_host("us-east-1", Some("http://127.0.0.1:5000/path/to/service")),
            "127.0.0.1:5000"
        );
    }

    /// A token must never be advertised as valid past the credentials that signed it, nor past the
    /// advertised cap.
    #[test]
    fn lifetime_clamped_to_credential_expiry() {
        // Credentials expiring 300s after signing time — earlier than the advertised 540s.
        let cred_expiry = fixed_time() + Duration::from_secs(300);
        let creds = Credentials::new(
            ACCESS_KEY,
            SECRET_KEY,
            Some(SESSION_TOKEN.to_string()),
            Some(cred_expiry),
            "test",
        );
        let out = build_token(&region(), HOST, &creds, fixed_time()).expect("token");
        assert_eq!(
            out.lifetime_ms,
            FIXED_EPOCH_SECS.cast_signed() * 1000 + 300_000
        );

        // Credentials expiring between the cap and the presign window (600s) -> the 540s cap.
        let medium = Credentials::new(
            ACCESS_KEY,
            SECRET_KEY,
            Some(SESSION_TOKEN.to_string()),
            Some(fixed_time() + Duration::from_secs(600)),
            "test",
        );
        let out = build_token(&region(), HOST, &medium, fixed_time()).expect("token");
        assert_eq!(
            out.lifetime_ms,
            FIXED_EPOCH_SECS.cast_signed() * 1000 + 540_000
        );

        // Credentials outliving the window -> the 540s cap.
        let long = Credentials::new(
            ACCESS_KEY,
            SECRET_KEY,
            Some(SESSION_TOKEN.to_string()),
            Some(fixed_time() + Duration::from_secs(3600)),
            "test",
        );
        let out = build_token(&region(), HOST, &long, fixed_time()).expect("token");
        assert_eq!(
            out.lifetime_ms,
            FIXED_EPOCH_SECS.cast_signed() * 1000 + 540_000
        );
    }

    /// The cache path: resolve via a static provider, then mint a token from the cached credentials.
    #[tokio::test]
    async fn credentials_cache_then_build_token() {
        let provider = SharedCredentialsProvider::new(creds(true));
        let cache = spawn_credentials_cache(provider, Duration::from_secs(5))
            .await
            .expect("cache");
        let cached = cache.lock().unwrap().clone();
        let out = build_token(&region(), HOST, &cached, fixed_time()).expect("token");
        let decoded = URL_SAFE_NO_PAD.decode(out.token.as_bytes()).unwrap();
        let url = String::from_utf8(decoded).unwrap();
        assert!(url.contains(GOLDEN_SIG_WITH_TOKEN));
    }

    /// A stalled credentials provider must surface a timeout, not hang the build.
    #[tokio::test(start_paused = true)]
    async fn credentials_load_times_out() {
        use aws_credential_types::provider::{ProvideCredentials, future};

        #[derive(Debug)]
        struct StalledProvider;
        impl ProvideCredentials for StalledProvider {
            fn provide_credentials<'a>(&'a self) -> future::ProvideCredentials<'a>
            where
                Self: 'a,
            {
                // Never resolves within the timeout window.
                future::ProvideCredentials::new(async {
                    tokio::time::sleep(Duration::from_secs(3600)).await;
                    unreachable!()
                })
            }
        }

        let provider = SharedCredentialsProvider::new(StalledProvider);
        let err = spawn_credentials_cache(provider, Duration::from_secs(5))
            .await
            .expect_err("must time out");
        assert!(
            matches!(err, MskIamError::CredentialsTimeout { .. }),
            "got: {err}"
        );
    }

    #[test]
    fn refresh_uses_skew_and_backoff() {
        // Expiry far out -> refresh ~skew before it.
        let far = SystemTime::now() + Duration::from_secs(3600);
        assert!(next_refresh(Some(far), false) >= Duration::from_secs(3600 - 5 * 60 - 5));
        // Inside the refresh window -> refresh right away instead of waiting out the floor.
        let near = SystemTime::now() + Duration::from_secs(30);
        assert_eq!(next_refresh(Some(near), false), Duration::ZERO);
        // After a failed or no-progress attempt -> back off, never zero.
        assert_eq!(next_refresh(Some(near), true), CRED_REFRESH_MIN);
        let past = SystemTime::now() - Duration::from_secs(10);
        assert_eq!(next_refresh(Some(past), true), CRED_REFRESH_MIN);
        // No expiry -> loose cadence.
        assert_eq!(next_refresh(None, false), CRED_REFRESH_NO_EXPIRY);
        assert_eq!(next_refresh(None, true), CRED_REFRESH_NO_EXPIRY);
    }

    /// A failed background refresh must be reported (it was silently dropped before), while the
    /// previous credentials stay in use and the refresh is retried after the back-off.
    #[tokio::test(start_paused = true)]
    async fn failed_refresh_is_reported_and_retried() {
        use aws_credential_types::provider::{ProvideCredentials, error::CredentialsError, future};
        use std::sync::atomic::{AtomicUsize, Ordering};

        #[derive(Debug)]
        struct FailingAfterFirstProvider(Arc<AtomicUsize>);
        impl ProvideCredentials for FailingAfterFirstProvider {
            fn provide_credentials<'a>(&'a self) -> future::ProvideCredentials<'a>
            where
                Self: 'a,
            {
                // First load: 30s left, so the refresher reloads right away; then every load fails.
                if self.0.fetch_add(1, Ordering::SeqCst) == 0 {
                    future::ProvideCredentials::ready(Ok(Credentials::new(
                        ACCESS_KEY,
                        SECRET_KEY,
                        None,
                        Some(SystemTime::now() + Duration::from_secs(30)),
                        "test",
                    )))
                } else {
                    future::ProvideCredentials::ready(Err(CredentialsError::provider_error("boom")))
                }
            }
        }

        vector_lib::event_test_util::clear_recorded_events();
        let calls = Arc::new(AtomicUsize::new(0));
        let provider =
            SharedCredentialsProvider::new(FailingAfterFirstProvider(Arc::clone(&calls)));
        let cache = spawn_credentials_cache(provider, Duration::from_secs(5))
            .await
            .expect("initial load");
        let initial_expiry = cache.lock().unwrap().expiry();

        tokio::time::sleep(Duration::from_secs(1)).await;
        assert_eq!(
            calls.load(Ordering::SeqCst),
            2,
            "refresh attempted right away"
        );
        vector_lib::event_test_util::contains_name_once("KafkaMskIamCredentialsRefreshError")
            .expect("failed refresh must be reported");
        assert_eq!(
            cache.lock().unwrap().expiry(),
            initial_expiry,
            "previous credentials are kept"
        );

        // Retried after the back-off, not in a tight loop.
        tokio::time::sleep(CRED_REFRESH_MIN).await;
        assert_eq!(
            calls.load(Ordering::SeqCst),
            3,
            "retried once after the back-off"
        );
    }

    /// The refresher must run in the component's span, so the HTTP telemetry of credential
    /// refreshes (STS/IMDS/profile) keeps its component labels like the initial load does.
    #[test]
    fn refresher_runs_in_the_component_span() {
        use aws_credential_types::provider::{ProvideCredentials, future};
        use std::sync::atomic::{AtomicUsize, Ordering};
        use tracing::Instrument;

        #[derive(Debug)]
        struct SpanRecordingProvider {
            calls: AtomicUsize,
            spans: Arc<Mutex<Vec<Option<&'static str>>>>,
        }
        impl ProvideCredentials for SpanRecordingProvider {
            fn provide_credentials<'a>(&'a self) -> future::ProvideCredentials<'a>
            where
                Self: 'a,
            {
                self.spans.lock().unwrap().push(
                    tracing::Span::current()
                        .metadata()
                        .map(tracing::Metadata::name),
                );
                // First load: 30s left, so the refresher reloads right away.
                let ttl = if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
                    30
                } else {
                    3600
                };
                future::ProvideCredentials::ready(Ok(Credentials::new(
                    ACCESS_KEY,
                    SECRET_KEY,
                    None,
                    Some(SystemTime::now() + Duration::from_secs(ttl)),
                    "test",
                )))
            }
        }

        let spans = Arc::new(Mutex::new(Vec::new()));
        let provider = SharedCredentialsProvider::new(SpanRecordingProvider {
            calls: AtomicUsize::new(0),
            spans: Arc::clone(&spans),
        });
        tracing::subscriber::with_default(tracing_subscriber::registry(), || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_time()
                .start_paused(true)
                .build()
                .unwrap();
            runtime.block_on(async {
                let _cache = spawn_credentials_cache(provider, Duration::from_secs(5))
                    .instrument(tracing::info_span!("kafka_component"))
                    .await
                    .expect("initial load");
                tokio::time::sleep(Duration::from_secs(1)).await;
            });
        });
        assert_eq!(
            *spans.lock().unwrap(),
            [Some("kafka_component"), Some("kafka_component")],
            "initial load and refresh must both run in the component span"
        );
    }

    /// Credentials that are already close to expiry must be replaced before they lapse, not after
    /// the one-minute back-off.
    #[tokio::test(start_paused = true)]
    async fn near_expiry_credentials_are_refreshed_immediately() {
        use aws_credential_types::provider::{ProvideCredentials, future};
        use std::sync::atomic::{AtomicUsize, Ordering};

        #[derive(Debug)]
        struct RotatingProvider(AtomicUsize);
        impl ProvideCredentials for RotatingProvider {
            fn provide_credentials<'a>(&'a self) -> future::ProvideCredentials<'a>
            where
                Self: 'a,
            {
                // First load: 30s left (inside the refresh window). Afterwards: a fresh hour.
                let ttl = if self.0.fetch_add(1, Ordering::SeqCst) == 0 {
                    30
                } else {
                    3600
                };
                future::ProvideCredentials::ready(Ok(Credentials::new(
                    ACCESS_KEY,
                    SECRET_KEY,
                    None,
                    Some(SystemTime::now() + Duration::from_secs(ttl)),
                    "test",
                )))
            }
        }

        let provider = SharedCredentialsProvider::new(RotatingProvider(AtomicUsize::new(0)));
        let cache = spawn_credentials_cache(provider, Duration::from_secs(5))
            .await
            .expect("initial load");
        // Well under the back-off: the refresher must already have swapped in fresh credentials.
        tokio::time::sleep(Duration::from_secs(1)).await;
        let expiry = cache.lock().unwrap().expiry().expect("expiry");
        assert!(
            expiry > SystemTime::now() + Duration::from_secs(600),
            "near-expiry credentials were not refreshed immediately"
        );
    }
}
