use crate::tls::TlsConfig;
use nkeys::error::Error as NKeysError;
use serde::{Deserialize, Serialize};
use snafu::Snafu;

#[derive(Debug, Snafu)]
pub enum NatsConfigError {
    #[snafu(display("NATS Auth Config Error: {}", source))]
    AuthConfigError { source: NKeysError },
    #[snafu(display("NATS TLS Config Error: missing key"))]
    TlsMissingKey,
    #[snafu(display("NATS TLS Config Error: missing cert"))]
    TlsMissingCert,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "strategy")]
pub enum NatsAuthConfig {
    UserPassword { user: String, password: String },
    Token { token: String },
    CredentialsFile { path: String },
    NKey { nkey: String, seed: String },
}

impl NatsAuthConfig {
    pub fn to_nats_options(&self) -> Result<async_nats::Options, NatsConfigError> {
        match self {
            NatsAuthConfig::UserPassword { user, password } => {
                Ok(async_nats::Options::with_user_pass(user, password))
            }
            NatsAuthConfig::CredentialsFile { path } => {
                Ok(async_nats::Options::with_credentials(path))
            }
            NatsAuthConfig::NKey { nkey, seed } => {
                let kp = nkeys::KeyPair::from_seed(seed)
                    .map_err(|e| NatsConfigError::AuthConfigError { source: e })?;
                // The following unwrap is safe because the only way the sign method can fail is if
                // keypair does not contain a seed. We are constructing the keypair from a seed in
                // the preceding line.
                Ok(async_nats::Options::with_nkey(nkey, move |nonce| {
                    kp.sign(nonce).unwrap()
                }))
            }
            NatsAuthConfig::Token { token } => Ok(async_nats::Options::with_token(token)),
        }
    }
}

pub fn from_tls_auth_config(
    connection_name: &str,
    auth_config: &Option<NatsAuthConfig>,
    tls_config: &Option<TlsConfig>,
) -> Result<async_nats::Options, NatsConfigError> {
    let nats_options = match &auth_config {
        None => async_nats::Options::new(),
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
