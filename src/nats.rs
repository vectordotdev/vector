use nkeys::error::Error as NKeysError;
use snafu::{ResultExt, Snafu};
use vector_lib::configurable::configurable_component;
use vector_lib::sensitive_string::SensitiveString;

use crate::tls::TlsEnableableConfig;

#[derive(Debug, Snafu)]
pub enum NatsConfigError {
    #[snafu(display("NATS Auth Config Error: {}", source))]
    AuthConfigError { source: NKeysError },
    #[snafu(display("NATS TLS Config Error: missing key"))]
    TlsMissingKey,
    #[snafu(display("NATS TLS Config Error: missing cert"))]
    TlsMissingCert,
    #[snafu(display("NATS Credentials file error"))]
    CredentialsFileError { source: std::io::Error },
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
    pub(crate) fn to_nats_options(&self) -> Result<async_nats::ConnectOptions, NatsConfigError> {
        match self {
            NatsAuthConfig::UserPassword { user_password } => {
                Ok(async_nats::ConnectOptions::with_user_and_password(
                    user_password.user.clone(),
                    user_password.password.inner().to_string(),
                ))
            }
            NatsAuthConfig::CredentialsFile { credentials_file } => {
                async_nats::ConnectOptions::with_credentials(
                    &std::fs::read_to_string(credentials_file.path.clone())
                        .context(CredentialsFileSnafu)?,
                )
                .context(CredentialsFileSnafu)
            }
            NatsAuthConfig::Nkey { nkey } => {
                Ok(async_nats::ConnectOptions::with_nkey(nkey.seed.clone()))
            }
            NatsAuthConfig::Token { token } => Ok(async_nats::ConnectOptions::with_token(
                token.value.inner().to_string(),
            )),
        }
    }
}

pub(crate) fn from_tls_auth_config(
    connection_name: &str,
    auth_config: &Option<NatsAuthConfig>,
    tls_config: &Option<TlsEnableableConfig>,
) -> Result<async_nats::ConnectOptions, NatsConfigError> {
    let nats_options = match &auth_config {
        None => async_nats::ConnectOptions::new(),
        Some(auth) => auth.to_nats_options()?,
    };

    let nats_options = nats_options.name(connection_name);

    match tls_config {
        None => Ok(nats_options),
        Some(tls_config) => {
            let tls_enabled = tls_config.enabled.unwrap_or(false);
            let nats_options = nats_options.require_tls(tls_enabled);
            if !tls_enabled {
                return Ok(nats_options);
            }

            let nats_options = match &tls_config.options.ca_file {
                None => nats_options,
                Some(ca_file) => nats_options.add_root_certificates(ca_file.clone()),
            };

            let nats_options = match (&tls_config.options.crt_file, &tls_config.options.key_file) {
                (None, None) => nats_options,
                (Some(crt_file), Some(key_file)) => {
                    nats_options.add_client_certificate(crt_file.clone(), key_file.clone())
                }
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

    fn parse_auth(s: &str) -> Result<async_nats::ConnectOptions, crate::Error> {
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
            credentials_file.path = "tests/data/nats/nats.creds"
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
