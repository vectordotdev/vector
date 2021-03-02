use super::AwsCredentialsProvider;
use rusoto_core::Region;
use serde::{Deserialize, Serialize};

/// Configuration for configuring authentication strategy for AWS.
#[derive(Serialize, Deserialize, Clone, Debug, Derivative)]
#[derivative(Default)]
#[serde(untagged)]
#[serde(deny_unknown_fields)]
pub enum AWSAuthentication {
    Static {
        access_key_id: String,
        secret_access_key: String,
    },
    Role {
        assume_role: String,
    },
    // Default variant is used instead of Option<AWSAuthentication> since even for
    // None we need to build `AwsCredentialsProvider`.
    //
    // {} is required to work around a bug in serde. https://github.com/serde-rs/serde/issues/1374
    #[derivative(Default)]
    Default {},
}

impl AWSAuthentication {
    pub fn build(
        &self,
        region: &Region,
        old_assume_role: Option<String>,
    ) -> crate::Result<AwsCredentialsProvider> {
        if old_assume_role.is_some() {
            warn!("Option `assume_role` has been renamed to `auth.assume_role`. Please use that one instead.");
        }
        match self {
            Self::Static {
                access_key_id,
                secret_access_key,
            } => {
                if old_assume_role.is_some() {
                    warn!("Ignoring option `assume_role`, instead using access options.");
                }
                Ok(AwsCredentialsProvider::new_minimal(
                    access_key_id,
                    secret_access_key,
                ))
            }
            Self::Role { assume_role } => {
                if old_assume_role.is_some() {
                    warn!(
                        "Ignoring option `assume_role`, instead using option `auth.assume_role`."
                    );
                }
                AwsCredentialsProvider::new(region, Some(assume_role.clone()))
            }
            Self::Default {} => AwsCredentialsProvider::new(region, old_assume_role),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize, Deserialize, Clone, Debug)]
    struct ComponentConfig {
        assume_role: Option<String>,
        #[serde(default)]
        auth: AWSAuthentication,
    }

    #[test]
    fn parsing_default() {
        let config = toml::from_str::<ComponentConfig>(
            r#"
        "#,
        )
        .unwrap();

        assert!(matches!(config.auth, AWSAuthentication::Default {}));
    }

    #[test]
    fn parsing_old_assume_role() {
        let config = toml::from_str::<ComponentConfig>(
            r#"
            assume_role = "root"
        "#,
        )
        .unwrap();

        assert!(matches!(config.auth, AWSAuthentication::Default {}));
    }

    #[test]
    fn parsing_assume_role() {
        let config = toml::from_str::<ComponentConfig>(
            r#"
            auth.assume_role = "root"
        "#,
        )
        .unwrap();

        assert!(matches!(config.auth, AWSAuthentication::Role { .. }));
    }

    #[test]
    fn parsing_both_assume_role() {
        let config = toml::from_str::<ComponentConfig>(
            r#"
            assume_role = "root"
            auth.assume_role = "auth.root"
        "#,
        )
        .unwrap();

        match config.auth {
            AWSAuthentication::Role { assume_role } => assert_eq!(&assume_role, "auth.root"),
            _ => panic!(),
        }
    }

    #[test]
    fn parsing_static() {
        let config = toml::from_str::<ComponentConfig>(
            r#"
            auth.access_key_id = "key"
            auth.secret_access_key = "other"
        "#,
        )
        .unwrap();

        assert!(matches!(config.auth, AWSAuthentication::Static { .. }));
    }
}
