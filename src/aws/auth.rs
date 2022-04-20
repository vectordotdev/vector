use aws_config::{
    default_provider::credentials::DefaultCredentialsChain, sts::AssumeRoleProviderBuilder,
};
use aws_types::{credentials::SharedCredentialsProvider, region::Region, Credentials};
use serde::{Deserialize, Serialize};

/// Configuration for configuring authentication strategy for AWS.
#[derive(Serialize, Deserialize, Clone, Debug, Derivative)]
#[derivative(Default)]
#[serde(untagged)]
#[serde(deny_unknown_fields)]
pub enum AwsAuthentication {
    Static {
        access_key_id: String,
        secret_access_key: String,
    },
    File {
        credentials_file: String,
        profile: Option<String>,
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

impl AwsAuthentication {
    pub async fn credentials_provider(
        &self,
        region: Option<Region>,
    ) -> crate::Result<SharedCredentialsProvider> {
        match self {
            Self::Static {
                access_key_id,
                secret_access_key,
            } => Ok(SharedCredentialsProvider::new(Credentials::from_keys(
                access_key_id,
                secret_access_key,
                None,
            ))),
            AwsAuthentication::File { .. } => {
                Err("Overriding the credentials file is not supported.".into())
            }
            AwsAuthentication::Role { assume_role } => {
                let mut credentials = AssumeRoleProviderBuilder::new(assume_role);

                if let Some(ref region) = region {
                    credentials = credentials.region(region.clone())
                }

                Ok(SharedCredentialsProvider::new(
                    credentials.build(default_credentials_provider(region).await),
                ))
            }
            AwsAuthentication::Default {} => Ok(SharedCredentialsProvider::new(
                default_credentials_provider(region).await,
            )),
        }
    }

    #[cfg(test)]
    pub fn test_auth() -> AwsAuthentication {
        AwsAuthentication::Static {
            access_key_id: "dummy".to_string(),
            secret_access_key: "dummy".to_string(),
        }
    }
}

async fn default_credentials_provider(region: Option<Region>) -> SharedCredentialsProvider {
    let mut credentials = DefaultCredentialsChain::builder();

    if let Some(region) = region {
        credentials = credentials.region(region)
    }

    SharedCredentialsProvider::new(credentials.build().await)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize, Deserialize, Clone, Debug)]
    struct ComponentConfig {
        assume_role: Option<String>,
        #[serde(default)]
        auth: AwsAuthentication,
    }

    #[test]
    fn parsing_default() {
        let config = toml::from_str::<ComponentConfig>(
            r#"
        "#,
        )
        .unwrap();

        assert!(matches!(config.auth, AwsAuthentication::Default {}));
    }

    #[test]
    fn parsing_old_assume_role() {
        let config = toml::from_str::<ComponentConfig>(
            r#"
            assume_role = "root"
        "#,
        )
        .unwrap();

        assert!(matches!(config.auth, AwsAuthentication::Default {}));
    }

    #[test]
    fn parsing_assume_role() {
        let config = toml::from_str::<ComponentConfig>(
            r#"
            auth.assume_role = "root"
        "#,
        )
        .unwrap();

        assert!(matches!(config.auth, AwsAuthentication::Role { .. }));
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
            AwsAuthentication::Role { assume_role } => assert_eq!(&assume_role, "auth.root"),
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

        assert!(matches!(config.auth, AwsAuthentication::Static { .. }));
    }

    #[test]
    fn parsing_file() {
        let config = toml::from_str::<ComponentConfig>(
            r#"
            auth.credentials_file = "/path/to/file"
            auth.profile = "foo"
        "#,
        )
        .unwrap();

        match config.auth {
            AwsAuthentication::File {
                credentials_file,
                profile,
            } => {
                assert_eq!(&credentials_file, "/path/to/file");
                assert_eq!(&profile.unwrap(), "foo");
            }
            _ => panic!(),
        }

        let config = toml::from_str::<ComponentConfig>(
            r#"
            auth.credentials_file = "/path/to/file"
        "#,
        )
        .unwrap();

        match config.auth {
            AwsAuthentication::File {
                credentials_file,
                profile,
            } => {
                assert_eq!(&credentials_file, "/path/to/file");
                assert_eq!(profile, None);
            }
            _ => panic!(),
        }
    }
}
