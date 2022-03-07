use crate::tls::TlsConfig;
use nkeys::error::Error as NKeysError;
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};

#[derive(Debug, Snafu)]
pub enum NatsConfigError {
    #[snafu(display("NATS Auth Config Error: {}", source))]
    AuthConfigError { source: NKeysError },
    #[snafu(display("NATS TLS Config Error: missing key"))]
    TlsMissingKey,
    #[snafu(display("NATS TLS Config Error: missing cert"))]
    TlsMissingCert,
    #[snafu(display("Missing configuration for auth strategy: {}", strategy))]
    AuthStrategyMissingConfiguration { strategy: NatsAuthStrategy },
}

#[derive(Derivative, Copy, Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
#[derivative(Default)]
pub enum NatsAuthStrategy {
    #[derivative(Default)]
    UserPassword,
    Token,
    CredentialsFile,
    NKey,
}

impl std::fmt::Display for NatsAuthStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        use NatsAuthStrategy::*;
        match self {
            UserPassword => write!(f, "user_password"),
            Token => write!(f, "token"),
            CredentialsFile => write!(f, "credentials_file"),
            NKey => write!(f, "nkey"),
        }
    }
}

#[derive(Default, Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "strategy")]
pub(crate) struct NatsAuthConfig {
    pub(crate) strategy: NatsAuthStrategy,
    pub(crate) user_password: Option<NatsAuthUserPassword>,
    pub(crate) token: Option<NatsAuthToken>,
    pub(crate) credentials_file: Option<NatsAuthCredentialsFile>,
    pub(crate) nkey: Option<NatsAuthNKey>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NatsAuthUserPassword {
    pub(crate) user: String,
    pub(crate) password: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NatsAuthToken {
    pub(crate) value: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NatsAuthCredentialsFile {
    pub(crate) path: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NatsAuthNKey {
    pub(crate) nkey: String,
    pub(crate) seed: String,
}

impl NatsAuthConfig {
    pub(crate) fn to_nats_options(&self) -> Result<nats::asynk::Options, NatsConfigError> {
        match self.strategy {
            NatsAuthStrategy::UserPassword => self
                .user_password
                .as_ref()
                .map(|config| nats::asynk::Options::with_user_pass(&config.user, &config.password))
                .ok_or(NatsConfigError::AuthStrategyMissingConfiguration {
                    strategy: self.strategy,
                }),
            NatsAuthStrategy::CredentialsFile => self
                .credentials_file
                .as_ref()
                .map(|config| nats::asynk::Options::with_credentials(&config.path))
                .ok_or(NatsConfigError::AuthStrategyMissingConfiguration {
                    strategy: self.strategy,
                }),
            NatsAuthStrategy::NKey => self
                .nkey
                .as_ref()
                .map(|config| {
                    nkeys::KeyPair::from_seed(&config.seed)
                        .context(AuthConfigSnafu)
                        .map(|kp| {
                            // The following unwrap is safe because the only way the sign method can fail is if
                            // keypair does not contain a seed. We are constructing the keypair from a seed in
                            // the preceding line.
                            nats::asynk::Options::with_nkey(&config.nkey, move |nonce| {
                                kp.sign(nonce).unwrap()
                            })
                        })
                })
                .ok_or(NatsConfigError::AuthStrategyMissingConfiguration {
                    strategy: self.strategy,
                })
                .and_then(std::convert::identity),
            NatsAuthStrategy::Token => self
                .token
                .as_ref()
                .map(|config| nats::asynk::Options::with_token(&config.value))
                .ok_or(NatsConfigError::AuthStrategyMissingConfiguration {
                    strategy: self.strategy,
                }),
        }
    }
}

pub(crate) fn from_tls_auth_config(
    connection_name: &str,
    auth_config: &Option<NatsAuthConfig>,
    tls_config: &Option<TlsConfig>,
) -> Result<nats::asynk::Options, NatsConfigError> {
    let nats_options = match &auth_config {
        None => nats::asynk::Options::new(),
        Some(auth) => auth.to_nats_options()?,
    };

    let nats_options = nats_options
        .with_name(connection_name)
        // Set reconnect_buffer_size on the nats client to 0 bytes so that the
        // client doesn't buffer internally (to avoid message loss).
        .reconnect_buffer_size(0);

    match tls_config {
        None => Ok(nats_options),
        Some(tls_config) => {
            let tls_enabled = tls_config.enabled.unwrap_or(false);
            let nats_options = nats_options.tls_required(tls_enabled);
            if !tls_enabled {
                return Ok(nats_options);
            }

            let nats_options = match &tls_config.options.ca_file {
                None => nats_options,
                Some(ca_file) => nats_options.add_root_certificate(ca_file),
            };

            let nats_options = match (&tls_config.options.crt_file, &tls_config.options.key_file) {
                (None, None) => nats_options,
                (Some(crt_file), Some(key_file)) => nats_options.client_cert(crt_file, key_file),
                (Some(_crt_file), None) => return Err(NatsConfigError::TlsMissingKey),
                (None, Some(_key_file)) => return Err(NatsConfigError::TlsMissingCert),
            };
            Ok(nats_options)
        }
    }
}
