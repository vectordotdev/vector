#![allow(missing_docs)]
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use aws_config::default_provider::credentials::DefaultCredentialsChain;
use aws_types::{region::Region, sdk_config::SharedCredentialsProvider};
#[cfg(feature = "sinks-kafka")]
use rdkafka::producer::{DeliveryResult, ProducerContext};
use rdkafka::{
    ClientConfig, ClientContext, Statistics, client::OAuthToken, consumer::ConsumerContext,
};
use snafu::Snafu;
use tokio::{runtime::Handle, sync::OnceCell};
use tracing::Span;
use vector_lib::{configurable::configurable_component, sensitive_string::SensitiveString};

use crate::{
    internal_events::KafkaStatisticsReceived,
    tls::{PEM_START_MARKER, TlsEnableableConfig},
};

#[derive(Debug, Snafu)]
enum KafkaError {
    #[snafu(display("invalid path: {:?}", path))]
    InvalidPath { path: PathBuf },
    #[snafu(display(
        "`msk_iam` cannot be combined with `sasl`; AWS MSK IAM authentication configures SASL automatically"
    ))]
    MskIamSaslConflict,
    #[snafu(display("`msk_iam` requires TLS; `tls.enabled` must not be set to false"))]
    MskIamTlsRequired,
}

/// Supported compression types for Kafka.
#[configurable_component]
#[derive(Clone, Copy, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum KafkaCompression {
    /// No compression.
    #[default]
    None,

    /// Gzip.
    Gzip,

    /// Snappy.
    Snappy,

    /// LZ4.
    Lz4,

    /// Zstandard.
    Zstd,
}

/// Kafka authentication configuration.
#[configurable_component]
#[derive(Clone, Debug, Default)]
pub struct KafkaAuthConfig {
    #[configurable(derived)]
    pub(crate) sasl: Option<KafkaSaslConfig>,

    #[configurable(derived)]
    pub(crate) tls: Option<TlsEnableableConfig>,

    #[configurable(derived)]
    pub(crate) msk_iam: Option<KafkaMskIamConfig>,
}

/// Configuration for SASL authentication when interacting with Kafka.
#[configurable_component]
#[derive(Clone, Debug, Default)]
pub struct KafkaSaslConfig {
    /// Enables SASL authentication.
    ///
    /// Only `PLAIN`- and `SCRAM`-based mechanisms are supported when configuring SASL authentication using `sasl.*`. For
    /// other mechanisms, `librdkafka_options.*` must be used directly to configure other `librdkafka`-specific values.
    /// If using `sasl.kerberos.*` as an example, where `*` is `service.name`, `principal`, `kinit.md`, etc., then
    /// `librdkafka_options.*` as a result becomes `librdkafka_options.sasl.kerberos.service.name`,
    /// `librdkafka_options.sasl.kerberos.principal`, etc.
    ///
    /// See the [librdkafka documentation](https://github.com/edenhill/librdkafka/blob/master/CONFIGURATION.md) for details.
    ///
    /// SASL authentication is not supported on Windows.
    pub(crate) enabled: Option<bool>,

    /// The SASL username.
    #[configurable(metadata(docs::examples = "username"))]
    pub(crate) username: Option<String>,

    /// The SASL password.
    #[configurable(metadata(docs::examples = "password"))]
    pub(crate) password: Option<SensitiveString>,

    /// The SASL mechanism to use.
    #[configurable(metadata(docs::examples = "SCRAM-SHA-256"))]
    #[configurable(metadata(docs::examples = "SCRAM-SHA-512"))]
    pub(crate) mechanism: Option<String>,
}

/// Configuration for AWS MSK IAM authentication.
///
/// When set, Vector authenticates to the cluster using SASL `OAUTHBEARER` tokens signed with
/// the AWS credentials from the default credentials provider chain (environment variables,
/// shared credentials file, IMDS, IRSA, and so on).
///
/// AWS MSK IAM authentication requires TLS, so `tls` must not be disabled. It cannot be
/// combined with `sasl`.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct KafkaMskIamConfig {
    /// The AWS region of the MSK cluster.
    #[configurable(metadata(docs::examples = "us-west-2"))]
    pub(crate) region: String,
}

impl KafkaAuthConfig {
    /// Builds the OAuth token provider used by client contexts to generate AWS MSK IAM
    /// authentication tokens, if MSK IAM authentication is configured.
    ///
    /// Must be called from within a Tokio runtime, whose handle is captured for use by the
    /// token generation callback (which librdkafka invokes from its own threads).
    pub(crate) fn msk_iam_token_provider(&self) -> Option<MskIamTokenProvider> {
        self.msk_iam.as_ref().map(|msk_iam| MskIamTokenProvider {
            region: Region::new(msk_iam.region.clone()),
            handle: Handle::current(),
            credentials_provider: Arc::default(),
            token_generated: Arc::default(),
        })
    }

    pub(crate) fn apply(&self, client: &mut ClientConfig) -> crate::Result<()> {
        let sasl_enabled = self.sasl.as_ref().and_then(|s| s.enabled).unwrap_or(false);
        let msk_iam_enabled = self.msk_iam.is_some();
        // MSK IAM requires TLS, so it is implied unless explicitly disabled (an error below).
        let tls_enabled = self
            .tls
            .as_ref()
            .and_then(|s| s.enabled)
            .unwrap_or(msk_iam_enabled);

        if msk_iam_enabled {
            if sasl_enabled {
                return Err(KafkaError::MskIamSaslConflict.into());
            }
            if !tls_enabled {
                return Err(KafkaError::MskIamTlsRequired.into());
            }
        }

        let protocol = match (sasl_enabled || msk_iam_enabled, tls_enabled) {
            (false, false) => "plaintext",
            (false, true) => "ssl",
            (true, false) => "sasl_plaintext",
            (true, true) => "sasl_ssl",
        };
        client.set("security.protocol", protocol);

        if msk_iam_enabled {
            client.set("sasl.mechanism", "OAUTHBEARER");
        }

        if sasl_enabled {
            let sasl = self.sasl.as_ref().unwrap();
            if let Some(username) = &sasl.username {
                client.set("sasl.username", username.as_str());
            }
            if let Some(password) = &sasl.password {
                client.set("sasl.password", password.inner());
            }
            if let Some(mechanism) = &sasl.mechanism {
                client.set("sasl.mechanism", mechanism);
            }
        }

        if tls_enabled && let Some(tls) = self.tls.as_ref() {
            if let Some(verify_certificate) = &tls.options.verify_certificate {
                client.set(
                    "enable.ssl.certificate.verification",
                    verify_certificate.to_string(),
                );
            }

            if let Some(verify_hostname) = &tls.options.verify_hostname {
                client.set(
                    "ssl.endpoint.identification.algorithm",
                    if *verify_hostname { "https" } else { "none" },
                );
            }

            if let Some(path) = &tls.options.ca_file {
                let text = pathbuf_to_string(path)?;
                if text.contains(PEM_START_MARKER) {
                    client.set("ssl.ca.pem", text);
                } else {
                    client.set("ssl.ca.location", text);
                }
            }

            if let Some(path) = &tls.options.crt_file {
                let text = pathbuf_to_string(path)?;
                if text.contains(PEM_START_MARKER) {
                    client.set("ssl.certificate.pem", text);
                } else {
                    client.set("ssl.certificate.location", text);
                }
            }

            if let Some(path) = &tls.options.key_file {
                let text = pathbuf_to_string(path)?;
                if text.contains(PEM_START_MARKER) {
                    client.set("ssl.key.pem", text);
                } else {
                    client.set("ssl.key.location", text);
                }
            }

            if let Some(pass) = &tls.options.key_pass {
                client.set("ssl.key.password", pass);
            }
        }

        Ok(())
    }
}

fn pathbuf_to_string(path: &Path) -> crate::Result<&str> {
    path.to_str()
        .ok_or_else(|| KafkaError::InvalidPath { path: path.into() }.into())
}

/// Generates SASL `OAUTHBEARER` tokens for AWS MSK IAM authentication.
#[derive(Clone)]
pub(crate) struct MskIamTokenProvider {
    region: Region,
    handle: Handle,
    /// The AWS credentials provider, built on first use and reused across token refreshes.
    /// The default provider chain caches credentials internally, so reusing it avoids
    /// re-resolving credentials (IMDS, STS, and so on) from scratch on every refresh.
    credentials_provider: Arc<OnceCell<SharedCredentialsProvider>>,
    /// Whether a token has been generated successfully at least once, allowing callers that
    /// prime the initial token (the sink healthcheck) to know when to stop polling.
    token_generated: Arc<AtomicBool>,
}

/// Upper bound on the token lifetime advertised to librdkafka.
///
/// MSK IAM tokens are actually valid for 15 minutes, but librdkafka schedules its refresh at
/// 80% of the advertised lifetime, so advertising the full lifetime leaves only ~3 minutes of
/// real validity by refresh time. During SASL re-authentication (`connections.max.reauth.ms`)
/// MSK rejects a token with too little remaining lifetime as "Session too short", causing
/// reconnect churn. Advertising a shorter lifetime makes librdkafka refresh sooner (~80% of
/// this bound), keeping the live token well clear of its true expiry with ample re-auth headroom.
const MAX_ADVERTISED_TOKEN_LIFETIME: Duration = Duration::from_secs(9 * 60);

impl MskIamTokenProvider {
    /// Generates a fresh MSK IAM authentication token.
    ///
    /// librdkafka invokes the token refresh callback either from one of its own threads or from
    /// the thread polling the client queue, which may be a Tokio worker thread. Blocking on the
    /// async token generation directly could panic or stall the runtime, so it is run on a
    /// short-lived thread instead. librdkafka refreshes tokens at 80% of their advertised
    /// lifetime, which is capped to [`MAX_ADVERTISED_TOKEN_LIFETIME`], so this is infrequent.
    fn token(&self) -> Result<OAuthToken, Box<dyn std::error::Error>> {
        // Bounds the whole operation: default-chain credential resolution plus signing. Sized
        // to absorb a slow credential source on cold start (for example an EKS Pod Identity or
        // IMDS agent) without hanging the sink healthcheck indefinitely.
        const TOKEN_GENERATION_TIMEOUT: Duration = Duration::from_secs(30);

        let region = self.region.clone();
        let handle = self.handle.clone();
        let credentials_provider = Arc::clone(&self.credentials_provider);
        let (token, expiration_time_ms) = std::thread::spawn(move || {
            handle.block_on(async {
                tokio::time::timeout(TOKEN_GENERATION_TIMEOUT, async {
                    let credentials_provider = credentials_provider
                        .get_or_init(|| {
                            let region = region.clone();
                            async move {
                                SharedCredentialsProvider::new(
                                    DefaultCredentialsChain::builder()
                                        .region(region)
                                        .build()
                                        .await,
                                )
                            }
                        })
                        .await
                        .clone();
                    aws_msk_iam_sasl_signer::generate_auth_token_from_credentials_provider(
                        region,
                        credentials_provider,
                    )
                    .await
                })
                .await
            })
        })
        .join()
        .map_err(|_| "MSK IAM token generation thread panicked")???;

        self.token_generated.store(true, Ordering::Release);

        Ok(OAuthToken {
            token,
            principal_name: String::new(),
            lifetime_ms: cap_token_lifetime(expiration_time_ms),
        })
    }

    /// Whether this provider has successfully generated a token at least once.
    #[cfg(feature = "sinks-kafka")]
    pub(crate) fn token_generated(&self) -> bool {
        self.token_generated.load(Ordering::Acquire)
    }
}

/// Caps the token expiry (absolute milliseconds since the Unix epoch, as returned by the AWS
/// signer and expected by librdkafka) so the advertised lifetime never exceeds
/// [`MAX_ADVERTISED_TOKEN_LIFETIME`]. Returns the real expiry unchanged if it is already sooner,
/// or if the system clock is somehow before the Unix epoch (in which case capping would
/// misfire and constant re-authentication is worse than no cap).
fn cap_token_lifetime(expiration_time_ms: i64) -> i64 {
    let Ok(now) = SystemTime::now().duration_since(UNIX_EPOCH) else {
        return expiration_time_ms;
    };
    let capped_expiry =
        (now.as_millis() as i64).saturating_add(MAX_ADVERTISED_TOKEN_LIFETIME.as_millis() as i64);
    expiration_time_ms.min(capped_expiry)
}

/// Generates an MSK IAM OAuth token via the given provider, shared by the client contexts
/// implementing the `generate_oauth_token` callback.
fn msk_iam_oauth_token(
    provider: Option<&MskIamTokenProvider>,
) -> Result<OAuthToken, Box<dyn std::error::Error>> {
    match provider {
        Some(provider) => provider.token(),
        None => Err("OAUTHBEARER authentication is only supported via `msk_iam`".into()),
    }
}

pub(crate) struct KafkaStatisticsContext {
    pub(crate) expose_lag_metrics: bool,
    pub span: Span,
    pub(crate) msk_iam_token_provider: Option<MskIamTokenProvider>,
}

impl ClientContext for KafkaStatisticsContext {
    // Enables handling of the token refresh event, which librdkafka only emits when the
    // `OAUTHBEARER` SASL mechanism is configured.
    const ENABLE_REFRESH_OAUTH_TOKEN: bool = true;

    fn stats(&self, statistics: Statistics) {
        // This callback get executed on a separate thread within the rdkafka library, so we need
        // to propagate the span here to attach the component tags to the emitted events.
        let _entered = self.span.enter();
        emit!(KafkaStatisticsReceived {
            statistics: &statistics,
            expose_lag_metrics: self.expose_lag_metrics,
        });
    }

    fn generate_oauth_token(
        &self,
        _oauthbearer_config: Option<&str>,
    ) -> Result<OAuthToken, Box<dyn std::error::Error>> {
        msk_iam_oauth_token(self.msk_iam_token_provider.as_ref())
    }
}

impl ConsumerContext for KafkaStatisticsContext {}

/// Client context for the sink healthcheck producer.
///
/// Serves MSK IAM authentication tokens like [`KafkaStatisticsContext`], but does not emit
/// statistics metrics: the healthcheck producer is short-lived and sends no data, so its
/// statistics would only pollute the component's metrics.
#[cfg(feature = "sinks-kafka")]
pub(crate) struct KafkaHealthcheckContext {
    pub(crate) msk_iam_token_provider: Option<MskIamTokenProvider>,
}

#[cfg(feature = "sinks-kafka")]
impl ClientContext for KafkaHealthcheckContext {
    const ENABLE_REFRESH_OAUTH_TOKEN: bool = KafkaStatisticsContext::ENABLE_REFRESH_OAUTH_TOKEN;

    // Ignore statistics rather than logging them like the default implementation does.
    fn stats(&self, _statistics: Statistics) {}

    fn generate_oauth_token(
        &self,
        _oauthbearer_config: Option<&str>,
    ) -> Result<OAuthToken, Box<dyn std::error::Error>> {
        msk_iam_oauth_token(self.msk_iam_token_provider.as_ref())
    }
}

// Required to use the context with a `BaseProducer` (the sink healthcheck); delivery reports
// are not consumed there.
#[cfg(feature = "sinks-kafka")]
impl ProducerContext for KafkaHealthcheckContext {
    type DeliveryOpaque = ();

    fn delivery(&self, _report: &DeliveryResult<'_>, _opaque: Self::DeliveryOpaque) {}
}

#[cfg(test)]
mod test {
    use super::*;

    fn msk_iam_config() -> KafkaMskIamConfig {
        KafkaMskIamConfig {
            region: "us-east-1".into(),
        }
    }

    #[test]
    fn msk_iam_configures_sasl_ssl_oauthbearer() {
        let auth = KafkaAuthConfig {
            sasl: None,
            tls: None,
            msk_iam: Some(msk_iam_config()),
        };
        let mut client = ClientConfig::new();
        auth.apply(&mut client).unwrap();
        assert_eq!(client.get("security.protocol"), Some("sasl_ssl"));
        assert_eq!(client.get("sasl.mechanism"), Some("OAUTHBEARER"));
    }

    #[test]
    fn msk_iam_implies_tls_with_tls_options() {
        let auth = KafkaAuthConfig {
            sasl: None,
            tls: Some(TlsEnableableConfig {
                enabled: None,
                ..Default::default()
            }),
            msk_iam: Some(msk_iam_config()),
        };
        let mut client = ClientConfig::new();
        auth.apply(&mut client).unwrap();
        assert_eq!(client.get("security.protocol"), Some("sasl_ssl"));
    }

    #[test]
    fn msk_iam_conflicts_with_sasl() {
        let auth = KafkaAuthConfig {
            sasl: Some(KafkaSaslConfig {
                enabled: Some(true),
                ..Default::default()
            }),
            tls: None,
            msk_iam: Some(msk_iam_config()),
        };
        let error = auth.apply(&mut ClientConfig::new()).unwrap_err();
        assert!(error.to_string().contains("cannot be combined"));
    }

    #[test]
    fn cap_token_lifetime_caps_far_future_expiry() {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        // A real MSK token expiring ~15 minutes out is capped to the advertised bound.
        let real_expiry = now_ms + 15 * 60 * 1000;
        let capped = cap_token_lifetime(real_expiry);
        assert!(capped < real_expiry);
        let advertised_lifetime = capped - now_ms;
        let bound = MAX_ADVERTISED_TOKEN_LIFETIME.as_millis() as i64;
        // Allow a small window for the clock advancing between the two `now` reads.
        assert!((advertised_lifetime - bound).abs() < 1000);
    }

    #[test]
    fn cap_token_lifetime_leaves_near_expiry_untouched() {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        // An expiry already sooner than the bound is returned unchanged.
        let real_expiry = now_ms + 60 * 1000;
        assert_eq!(cap_token_lifetime(real_expiry), real_expiry);
    }

    #[test]
    fn msk_iam_token_provider_generates_capped_token() {
        // MSK IAM token generation is entirely local: the "token" is a SigV4-presigned URL
        // for the `kafka-cluster:Connect` action, so static credentials from the environment
        // exercise the full generation path (thread spawn, credential chain, signer, lifetime
        // cap) without any AWS access.
        //
        // SAFETY: tests run under nextest, one process per test, so no other thread can be
        // reading the environment concurrently.
        unsafe {
            std::env::set_var("AWS_ACCESS_KEY_ID", "AKIAIOSFODNN7EXAMPLE");
            std::env::set_var(
                "AWS_SECRET_ACCESS_KEY",
                "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
            );
        }

        // A multi-threaded runtime, kept otherwise idle so its workers can drive the token
        // generation future that `token` blocks on from its own short-lived thread.
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let auth = KafkaAuthConfig {
            sasl: None,
            tls: None,
            msk_iam: Some(msk_iam_config()),
        };
        let provider = {
            let _guard = runtime.enter();
            auth.msk_iam_token_provider().unwrap()
        };
        assert!(!provider.token_generated.load(Ordering::Acquire));

        let token = provider.token().unwrap();

        // The token is the base64url-encoded presigned URL, so it starts with the encoding
        // of `https://kafk` (the longest prefix aligned to a whole base64 input group).
        assert!(
            token.token.starts_with("aHR0cHM6Ly9rYWZr"),
            "not a base64url-encoded MSK presigned URL: {}",
            token.token
        );

        // The signer returns a ~15 minute expiry, so the advertised expiry must have been
        // capped: still in the future, close to the cap, and never beyond it.
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let bound = MAX_ADVERTISED_TOKEN_LIFETIME.as_millis() as i64;
        assert!(token.lifetime_ms > now_ms);
        assert!(token.lifetime_ms <= now_ms + bound);
        // Allow for clocks advancing while the token was generated.
        assert!(token.lifetime_ms >= now_ms + bound - 30_000);

        assert!(provider.token_generated.load(Ordering::Acquire));
    }

    #[test]
    fn msk_iam_rejects_disabled_tls() {
        let auth = KafkaAuthConfig {
            sasl: None,
            tls: Some(TlsEnableableConfig {
                enabled: Some(false),
                ..Default::default()
            }),
            msk_iam: Some(msk_iam_config()),
        };
        let error = auth.apply(&mut ClientConfig::new()).unwrap_err();
        assert!(error.to_string().contains("requires TLS"));
    }
}
