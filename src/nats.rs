use nkeys::error::Error as NKeysError;
use snafu::{ResultExt, Snafu};
use vector_common::sensitive_string::SensitiveString;
use vector_config::configurable_component;

use crate::tls::TlsEnableableConfig;

#[derive(Debug, Snafu)]
pub enum NatsConfigError {
    #[snafu(display("NATS Auth Config Error: {}", source))]
    AuthConfigError { source: NKeysError },
    #[snafu(display("NATS TLS Config Error: missing key"))]
    TlsMissingKey,
    #[snafu(display("NATS TLS Config Error: missing cert"))]
    TlsMissingCert,
}

/// Configuration of the authentication strategy when interacting with NATS.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(rename_all = "snake_case", tag = "strategy")]
#[configurable(metadata(
    docs::enum_tag_description = "The strategy used to authenticate with the NATS server.

More information on NATS authentication, and the various authentication strategies, can be found in the
NATS [documentation][nats_auth_docs]. For TLS client certificate authentication specifically, see the
`tls` settings.

[nats_auth_docs]: https://docs.nats.io/running-a-nats-service/configuration/securing_nats/auth_intro"
))]
pub(crate) enum NatsAuthConfig {
    /// Username/password authentication.
    UserPassword {
        #[configurable(derived)]
        user_password: NatsAuthUserPassword,
    },

    /// Token authentication.
    Token {
        #[configurable(derived)]
        token: NatsAuthToken,
    },

    /// Credentials file authentication. (JWT-based)
    CredentialsFile {
        #[configurable(derived)]
        credentials_file: NatsAuthCredentialsFile,
    },

    /// NKey authentication.
    Nkey {
        #[configurable(derived)]
        nkey: NatsAuthNKey,
    },
}

impl std::fmt::Display for NatsAuthConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        use NatsAuthConfig::*;
        let word = match self {
            UserPassword { .. } => "user_password",
            Token { .. } => "token",
            CredentialsFile { .. } => "credentials_file",
            Nkey { .. } => "nkey",
        };
        write!(f, "{}", word)
    }
}

/// Username and password configuration.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub(crate) struct NatsAuthUserPassword {
    /// Username.
    pub(crate) user: String,

    /// Password.
    pub(crate) password: SensitiveString,
}

/// Token configuration.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub(crate) struct NatsAuthToken {
    /// Token.
    pub(crate) value: SensitiveString,
}

/// Credentials file configuration.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub(crate) struct NatsAuthCredentialsFile {
    /// Path to credentials file.
    #[configurable(metadata(docs::examples = "/etc/nats/nats.creds"))]
    pub(crate) path: String,
}

/// NKeys configuration.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub(crate) struct NatsAuthNKey {
    /// User.
    ///
    /// Conceptually, this is equivalent to a public key.
    pub(crate) nkey: String,

    /// Seed.
    ///
    /// Conceptually, this is equivalent to a private key.
    pub(crate) seed: String,
}

impl NatsAuthConfig {
    pub(crate) fn to_nats_options(&self) -> Result<nats::asynk::Options, NatsConfigError> {
        match self {
            NatsAuthConfig::UserPassword { user_password } => {
                Ok(nats::asynk::Options::with_user_pass(
                    user_password.user.as_str(),
                    user_password.password.inner(),
                ))
            }
            NatsAuthConfig::CredentialsFile { credentials_file } => Ok(
                nats::asynk::Options::with_credentials(&credentials_file.path),
            ),
            NatsAuthConfig::Nkey { nkey } => nkeys::KeyPair::from_seed(&nkey.seed)
                .context(AuthConfigSnafu)
                .map(|kp| {
                    // The following unwrap is safe because the only way the sign method can fail is if
                    // keypair does not contain a seed. We are constructing the keypair from a seed in
                    // the preceding line.
                    nats::asynk::Options::with_nkey(&nkey.nkey, move |nonce| {
                        kp.sign(nonce).unwrap()
                    })
                }),
            NatsAuthConfig::Token { token } => {
                Ok(nats::asynk::Options::with_token(token.value.inner()))
            }
        }
    }
}

pub(crate) fn from_tls_auth_config(
    connection_name: &str,
    auth_config: &Option<NatsAuthConfig>,
    tls_config: &Option<TlsEnableableConfig>,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_auth(s: &str) -> Result<nats::asynk::Options, crate::Error> {
        toml::from_str(s)
            .map_err(Into::into)
            .and_then(|config: NatsAuthConfig| config.to_nats_options().map_err(Into::into))
    }

    #[test]
    fn auth_user_password_ok() {
        parse_auth(
            r#"
            strategy = "user_password"
            user_password.user = "username"
            user_password.password = "password"
        "#,
        )
        .unwrap();
    }

    #[test]
    fn auth_user_password_missing_user() {
        parse_auth(
            r#"
            strategy = "user_password"
            user_password.password = "password"
        "#,
        )
        .unwrap_err();
    }

    #[test]
    fn auth_user_password_missing_password() {
        parse_auth(
            r#"
            strategy = "user_password"
            user_password.user = "username"
        "#,
        )
        .unwrap_err();
    }

    #[test]
    fn auth_user_password_missing_all() {
        parse_auth(
            r#"
            strategy = "user_password"
            token.value = "foobar"
            "#,
        )
        .unwrap_err();
    }

    #[test]
    fn auth_token_ok() {
        parse_auth(
            r#"
            strategy = "token"
            token.value = "token"
        "#,
        )
        .unwrap();
    }

    #[test]
    fn auth_token_missing() {
        parse_auth(
            r#"
            strategy = "token"
            user_password.user = "foobar"
            "#,
        )
        .unwrap_err();
    }

    #[test]
    fn auth_credentials_file_ok() {
        parse_auth(
            r#"
            strategy = "credentials_file"
            credentials_file.path = "/path/to/nowhere"
        "#,
        )
        .unwrap();
    }

    #[test]
    fn auth_credentials_file_missing() {
        parse_auth(
            r#"
            strategy = "credentials_file"
            token.value = "foobar"
            "#,
        )
        .unwrap_err();
    }

    #[test]
    fn auth_nkey_ok() {
        parse_auth(
            r#"
            strategy = "nkey"
            nkey.nkey = "UC435ZYS52HF72E2VMQF4GO6CUJOCHDUUPEBU7XDXW5AQLIC6JZ46PO5"
            nkey.seed = "SUAAEZYNLTEA2MDTG7L5X7QODZXYHPOI2LT2KH5I4GD6YVP24SE766EGPA"
        "#,
        )
        .unwrap();
    }

    #[test]
    fn auth_nkey_missing_nkey() {
        parse_auth(
            r#"
            strategy = "nkey"
            nkey.seed = "SUAAEZYNLTEA2MDTG7L5X7QODZXYHPOI2LT2KH5I4GD6YVP24SE766EGPA"
        "#,
        )
        .unwrap_err();
    }

    #[test]
    fn auth_nkey_missing_seed() {
        parse_auth(
            r#"
            strategy = "nkey"
            nkey.nkey = "UC435ZYS52HF72E2VMQF4GO6CUJOCHDUUPEBU7XDXW5AQLIC6JZ46PO5"
        "#,
        )
        .unwrap_err();
    }

    #[test]
    fn auth_nkey_missing_both() {
        parse_auth(
            r#"
            strategy = "nkey"
            user_password.user = "foobar"
            "#,
        )
        .unwrap_err();
    }
}
