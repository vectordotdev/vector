use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "strategy")]
pub(crate) enum NatsAuthConfig {
    UserPassword { user: String, password: String },
    Token { token: String },
    CredentialsFile { path: String },
    NKey { nkey: String, seed: String },
}

impl NatsAuthConfig {
    pub(crate) fn to_nats_options(&self) -> async_nats::Options {
        match self {
            NatsAuthConfig::UserPassword { user, password } => {
                async_nats::Options::with_user_pass(user, password)
            }
            NatsAuthConfig::CredentialsFile { path } => async_nats::Options::with_credentials(path),
            NatsAuthConfig::NKey { nkey, seed } => {
                let kp = nkeys::KeyPair::from_seed(seed).unwrap();
                async_nats::Options::with_nkey(nkey, move |nonce| kp.sign(nonce).unwrap())
            }
            NatsAuthConfig::Token { token } => async_nats::Options::with_token(token),
        }
    }
}
