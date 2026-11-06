//! SASL/OAUTHBEARER token providers for the `kafka` source.
//!
//! librdkafka asks the client context for a token when `sasl.mechanism` is `OAUTHBEARER` and
//! `sasl.oauthbearer.method` is not `oidc`. With `oidc`, librdkafka gets and refreshes its tokens
//! on its own background thread and never asks the client context.

use std::{collections::HashMap, error::Error, sync::Arc, time::Duration};

use futures::future::BoxFuture;
use rdkafka::client::OAuthToken;
use tokio::sync::oneshot;

use super::{KafkaAuthConfig, KafkaSaslAzureConfig, OAUTHBEARER};

/// The longest time that a component waits for the first token when it starts.
///
/// librdkafka asks for the first token when the Kafka client is created, and rust-rdkafka passes
/// that request to the client context at the first poll. If the provider has no token by then,
/// librdkafka reports an authentication error and asks again only 10 seconds later. So a component
/// waits for the first token before it polls the client, but not longer than this. After a
/// timeout, librdkafka asks again every 10 seconds until the provider has a token.
const FIRST_TOKEN_TIMEOUT: Duration = Duration::from_secs(10);

/// Supplies SASL/OAUTHBEARER tokens to librdkafka.
///
/// librdkafka asks for a token on the thread that polls the Kafka client. For the `kafka` source,
/// that thread runs inside the Tokio runtime, so `token` must return a token from memory. It must
/// not block or enter the runtime. A separate task keeps the token fresh.
pub(crate) trait OAuthTokenProvider: Send + Sync {
    /// Returns the current token, or an error that librdkafka logs before it asks again.
    ///
    /// The error text must not contain the token or other secrets.
    fn token(&self) -> Result<OAuthToken, Box<dyn Error>>;
}

/// A token provider that a Kafka client context can hold.
pub(crate) type SharedOAuthTokenProvider = Arc<dyn OAuthTokenProvider>;

/// Gets the first token of a provider and keeps it fresh. Nothing happens until
/// [`OAuthTokenRefresh::start`] runs.
pub(crate) struct OAuthTokenRefresh {
    /// Gets tokens for the provider, and ends when the provider is dropped.
    pub(super) task: BoxFuture<'static, ()>,
    /// Completes when the provider has its first token, or when `task` ends.
    pub(super) first_token: oneshot::Receiver<()>,
}

impl OAuthTokenRefresh {
    /// Spawns the task, then waits until the provider has its first token, the task ends, or
    /// [`FIRST_TOKEN_TIMEOUT`] passes. Call it when the component starts, before the first poll of
    /// the Kafka client.
    pub(crate) async fn start(self) {
        crate::spawn_in_current_span(self.task);
        _ = tokio::time::timeout(FIRST_TOKEN_TIMEOUT, self.first_token).await;
    }
}

/// A token provider and the refresh that keeps its token fresh.
type OAuthTokenProviderAndRefresh = (SharedOAuthTokenProvider, OAuthTokenRefresh);

/// Answers a token request from librdkafka with a token from `provider`.
pub(crate) fn generate_oauth_token(
    provider: Option<&dyn OAuthTokenProvider>,
) -> Result<OAuthToken, Box<dyn Error>> {
    provider
        .ok_or(
            "SASL/OAUTHBEARER needs a token source: set `sasl.azure`, or set \
             `sasl.oauthbearer.method` to `oidc` in `librdkafka_options`",
        )?
        .token()
}

impl KafkaAuthConfig {
    /// Creates the SASL/OAUTHBEARER token provider for `sasl.azure`, if SASL uses it, and the
    /// refresh that keeps its token fresh.
    ///
    /// This first checks the settings that `sasl.azure` depends on. It does not create the Azure
    /// credential or request a token: the refresh does that when it starts, and retries until it
    /// succeeds.
    pub(crate) fn oauth_token_provider(
        &self,
        librdkafka_options: Option<&HashMap<String, String>>,
    ) -> crate::Result<Option<OAuthTokenProviderAndRefresh>> {
        let Some(azure) = self.sasl_azure() else {
            return Ok(None);
        };
        self.check_sasl_azure(azure, librdkafka_options)?;
        azure_token_provider(azure).map(Some)
    }

    /// Checks the settings that `sasl.azure` depends on, and warns when TLS does not authenticate
    /// the broker. `KafkaAuthConfig::apply` relies on these checks.
    fn check_sasl_azure(
        &self,
        azure: &KafkaSaslAzureConfig,
        librdkafka_options: Option<&HashMap<String, String>>,
    ) -> crate::Result<()> {
        if !self
            .tls
            .as_ref()
            .and_then(|tls| tls.enabled)
            .unwrap_or(false)
        {
            return Err(
                "`sasl.azure` sends a bearer token, so it requires `tls.enabled: true`".into(),
            );
        }
        let sasl = self.sasl.as_ref();
        if let Some(mechanism) = sasl.and_then(|sasl| sasl.mechanism.as_ref())
            && !mechanism.eq_ignore_ascii_case(OAUTHBEARER)
        {
            return Err(format!(
                "`sasl.azure` uses the `{OAUTHBEARER}` SASL mechanism, but `sasl.mechanism` is \
                 `{mechanism}`"
            )
            .into());
        }
        if sasl.is_some_and(|sasl| sasl.username.is_some() || sasl.password.is_some()) {
            return Err(
                "`sasl.username` and `sasl.password` cannot be set with `sasl.azure`".into(),
            );
        }
        if azure.token_scope.trim().is_empty() {
            return Err("`sasl.azure.token_scope` cannot be empty".into());
        }
        check_librdkafka_options(librdkafka_options.into_iter().flat_map(HashMap::keys))?;
        if self.tls_verification_is_off(librdkafka_options) {
            warn!(
                "TLS does not verify the certificate or the host name of the broker, so the \
                 `sasl.azure` bearer token can go to a server that is not the broker."
            );
        }
        Ok(())
    }

    /// Returns whether TLS is on but does not check the certificate or the host name of the broker.
    /// `librdkafka_options` replace the `tls` settings.
    fn tls_verification_is_off(
        &self,
        librdkafka_options: Option<&HashMap<String, String>>,
    ) -> bool {
        let Some(tls) = self.tls.as_ref().filter(|tls| tls.enabled == Some(true)) else {
            return false;
        };
        let option = |key: &str| {
            librdkafka_options
                .and_then(|options| options.get(key))
                .map(|value| value.trim().to_ascii_lowercase())
        };
        let verify_certificate = option("enable.ssl.certificate.verification")
            .map_or(tls.options.verify_certificate != Some(false), |value| {
                !matches!(value.as_str(), "false" | "f" | "0")
            });
        let verify_hostname = option("ssl.endpoint.identification.algorithm")
            .map_or(tls.options.verify_hostname != Some(false), |value| {
                value != "none"
            });
        !(verify_certificate && verify_hostname)
    }
}

/// Rejects `librdkafka_options` keys that would replace the settings that `sasl.azure` needs, or
/// send its token to brokers that `bootstrap_servers` does not name.
///
/// The component sets these options on the client after `KafkaAuthConfig::apply`, so they would
/// replace its settings. For example, `sasl.oauthbearer.method=oidc` makes librdkafka get its own
/// tokens and never ask Vector, `enable.sasl.oauthbearer.unsecure.jwt` makes it create an unsigned
/// token, and `bootstrap.servers` (an alias of `metadata.broker.list`) replaces the brokers.
fn check_librdkafka_options<'a>(keys: impl IntoIterator<Item = &'a String>) -> crate::Result<()> {
    const RESERVED: [&str; 8] = [
        "bootstrap.servers",
        "metadata.broker.list",
        "security.protocol",
        "sasl.mechanism",
        "sasl.mechanisms",
        "sasl.username",
        "sasl.password",
        "enable.sasl.oauthbearer.unsecure.jwt",
    ];

    let mut conflicts: Vec<_> = keys
        .into_iter()
        .filter(|key| {
            let key = key.to_ascii_lowercase();
            RESERVED.contains(&key.as_str()) || key.starts_with("sasl.oauthbearer.")
        })
        .map(|key| format!("`{key}`"))
        .collect();
    if conflicts.is_empty() {
        return Ok(());
    }
    conflicts.sort_unstable();
    Err(format!(
        "`librdkafka_options` cannot set {} when `sasl.azure` is set",
        conflicts.join(", ")
    )
    .into())
}

#[cfg(feature = "azure-core")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "Without `azure-core`, this function returns an error."
)]
fn azure_token_provider(
    config: &KafkaSaslAzureConfig,
) -> crate::Result<OAuthTokenProviderAndRefresh> {
    // Creating some credentials reads files, for example the federated token file of a workload
    // identity, so the refresh creates the credential when it starts.
    let auth = config.auth.clone();
    let (provider, refresh) = super::azure::AzureTokenProvider::new(
        move || auth.credential(),
        config.token_scope.clone(),
    );
    let provider: SharedOAuthTokenProvider = Arc::new(provider);
    Ok((provider, refresh))
}

#[cfg(not(feature = "azure-core"))]
fn azure_token_provider(
    _config: &KafkaSaslAzureConfig,
) -> crate::Result<OAuthTokenProviderAndRefresh> {
    Err("`sasl.azure` requires Vector to be built with the `azure-core` feature".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    const AZURE_AUTH: &str = r#"
        sasl:
          enabled: true
          azure:
            token_scope: "https://mynamespace.servicebus.windows.net/.default"
            auth:
              azure_credential_kind: managed_identity
        tls:
          enabled: true
        "#;

    fn azure_auth() -> KafkaAuthConfig {
        serde_yaml::from_str(AZURE_AUTH).unwrap()
    }

    fn options(keys: &[&str]) -> HashMap<String, String> {
        keys.iter()
            .map(|key| ((*key).to_owned(), "value".to_owned()))
            .collect()
    }

    #[tokio::test(start_paused = true)]
    async fn start_stops_waiting_when_the_task_ends_without_a_token() {
        let (got_first_token, first_token) = oneshot::channel::<()>();
        let refresh = OAuthTokenRefresh {
            task: Box::pin(async move { drop(got_first_token) }),
            first_token,
        };
        let started = tokio::time::Instant::now();
        refresh.start().await;
        assert_eq!(started.elapsed(), Duration::ZERO);
    }

    #[test]
    fn sasl_azure_requires_tls() {
        let mut without_tls = azure_auth();
        without_tls.tls = None;
        let mut tls_disabled = azure_auth();
        tls_disabled.tls.as_mut().unwrap().enabled = Some(false);
        for auth in [without_tls, tls_disabled] {
            let error = auth.oauth_token_provider(None).err().unwrap().to_string();
            assert_eq!(
                error,
                "`sasl.azure` sends a bearer token, so it requires `tls.enabled: true`"
            );
        }
    }

    #[test]
    fn sasl_azure_accepts_only_the_oauthbearer_mechanism() {
        let mut auth = azure_auth();
        let azure = auth.sasl_azure().unwrap().clone();
        auth.sasl.as_mut().unwrap().mechanism = Some("oauthbearer".to_owned());
        auth.check_sasl_azure(&azure, None).unwrap();

        auth.sasl.as_mut().unwrap().mechanism = Some("SCRAM-SHA-512".to_owned());
        let error = auth.oauth_token_provider(None).err().unwrap().to_string();
        assert!(
            error.contains("`sasl.mechanism` is `SCRAM-SHA-512`"),
            "{error}"
        );
    }

    #[test]
    fn sasl_azure_rejects_username_and_password() {
        let mut with_username = azure_auth();
        with_username.sasl.as_mut().unwrap().username = Some("user".to_owned());
        let mut with_password = azure_auth();
        with_password.sasl.as_mut().unwrap().password = Some("secret".to_owned().into());
        for auth in [with_username, with_password] {
            let error = auth.oauth_token_provider(None).err().unwrap().to_string();
            assert!(
                error.contains("`sasl.username` and `sasl.password`"),
                "{error}"
            );
            assert!(!error.contains("secret"), "{error}");
        }
    }

    #[test]
    fn sasl_azure_rejects_an_empty_token_scope() {
        let mut auth = azure_auth();
        let azure = auth.sasl.as_mut().unwrap().azure.as_mut().unwrap();
        azure.token_scope = " ".to_owned();
        let error = auth
            .oauth_token_provider(None)
            .err()
            .expect("an empty token scope should be rejected")
            .to_string();
        assert!(error.contains("`sasl.azure.token_scope`"), "{error}");
    }

    #[test]
    fn librdkafka_options_that_replace_sasl_azure_settings_are_rejected() {
        for key in [
            "bootstrap.servers",
            "metadata.broker.list",
            "security.protocol",
            "SASL.MECHANISM",
            "sasl.mechanisms",
            "sasl.username",
            "sasl.password",
            "sasl.oauthbearer.method",
            "sasl.oauthbearer.config",
            "Sasl.OAuthBearer.Token.Endpoint.Url",
            "enable.sasl.oauthbearer.unsecure.jwt",
        ] {
            let error = azure_auth()
                .oauth_token_provider(Some(&options(&[key])))
                .err()
                .expect(key)
                .to_string();
            assert!(error.contains(&format!("`{key}`")), "{error}");
        }
    }

    #[test]
    fn librdkafka_option_conflicts_are_listed_in_order() {
        let error = check_librdkafka_options(
            options(&["sasl.password", "client.id", "security.protocol"]).keys(),
        )
        .unwrap_err()
        .to_string();
        assert_eq!(
            error,
            "`librdkafka_options` cannot set `sasl.password`, `security.protocol` when \
             `sasl.azure` is set"
        );
    }

    #[test]
    fn other_librdkafka_options_are_allowed() {
        check_librdkafka_options(
            options(&["client.id", "socket.keepalive.enable", "ssl.ca.location"]).keys(),
        )
        .unwrap();
    }

    #[test]
    fn librdkafka_options_pass_through_without_sasl_azure() {
        // librdkafka's built-in OIDC keeps working: it never asks the client context for a token.
        let oidc = options(&["sasl.oauthbearer.method", "sasl.oauthbearer.client.id"]);
        let auth: KafkaAuthConfig =
            serde_yaml::from_str("sasl:\n  enabled: true\n  mechanism: OAUTHBEARER\n").unwrap();
        assert!(auth.oauth_token_provider(Some(&oidc)).unwrap().is_none());

        let mut auth = azure_auth();
        auth.sasl.as_mut().unwrap().enabled = Some(false);
        assert!(auth.oauth_token_provider(Some(&oidc)).unwrap().is_none());
    }

    #[test]
    fn tls_verification_is_off_when_a_check_is_disabled() {
        const CERTIFICATE: &str = "enable.ssl.certificate.verification";
        const HOSTNAME: &str = "ssl.endpoint.identification.algorithm";
        for (verify_certificate, verify_hostname, librdkafka_options, off) in [
            (None, None, vec![], false),
            (Some(true), Some(true), vec![], false),
            (Some(false), None, vec![], true),
            (None, Some(false), vec![], true),
            (None, None, vec![(CERTIFICATE, "false")], true),
            (None, None, vec![(CERTIFICATE, "F")], true),
            (None, None, vec![(HOSTNAME, "None")], true),
            (
                None,
                None,
                vec![(CERTIFICATE, "true"), (HOSTNAME, "https")],
                false,
            ),
            // `librdkafka_options` replace the `tls` settings.
            (
                Some(false),
                Some(false),
                vec![(CERTIFICATE, "1"), (HOSTNAME, "https")],
                false,
            ),
        ] {
            let mut auth = azure_auth();
            let tls = auth.tls.as_mut().unwrap();
            tls.options.verify_certificate = verify_certificate;
            tls.options.verify_hostname = verify_hostname;
            let options = librdkafka_options
                .iter()
                .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
                .collect::<HashMap<_, _>>();
            assert_eq!(
                auth.tls_verification_is_off(Some(&options)),
                off,
                "{verify_certificate:?} {verify_hostname:?} {librdkafka_options:?}"
            );

            // Without TLS, `check_sasl_azure` rejects `sasl.azure` instead.
            auth.tls.as_mut().unwrap().enabled = Some(false);
            assert!(!auth.tls_verification_is_off(Some(&options)));
        }
    }

    #[cfg(not(feature = "azure-core"))]
    #[test]
    fn sasl_azure_requires_the_azure_core_feature() {
        let error = azure_auth()
            .oauth_token_provider(None)
            .err()
            .expect("`sasl.azure` should fail without the `azure-core` feature")
            .to_string();
        assert!(error.contains("`azure-core`"), "{error}");
    }

    #[cfg(feature = "azure-core")]
    mod azure {
        use super::*;

        #[test]
        fn sasl_azure_creates_a_provider_without_requesting_a_token() {
            // `sasl.azure` also enables SASL when `sasl.enabled` is not set.
            for enabled in [Some(true), None] {
                let mut auth = azure_auth();
                auth.sasl.as_mut().unwrap().enabled = enabled;
                let (provider, _refresh) = auth
                    .oauth_token_provider(Some(&options(&["client.id"])))
                    .unwrap()
                    .expect("`sasl.azure` should create a token provider");
                // The refresh task has not run, so the provider has no token yet.
                let error = provider
                    .token()
                    .err()
                    .expect("the provider should have no token before the refresh task runs")
                    .to_string();
                assert!(error.contains("not available yet"), "{error}");
            }
        }
    }
}
