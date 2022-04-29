use aws_config::{
    default_provider::credentials::DefaultCredentialsChain, sts::AssumeRoleProviderBuilder,
};
use aws_types::{credentials::SharedCredentialsProvider, region::Region, Credentials};
use serde::{Deserialize, Serialize};
use std::time::Duration;

// matches default load timeout from the SDK as of 0.10.1, but lets us confidently document the
// default rather than relying on the SDK default to not change
const DEFAULT_LOAD_TIMEOUT: Duration = Duration::from_secs(5);

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
        load_timeout_secs: Option<u64>,
    },
    // Default variant is used instead of Option<AWSAuthentication> since even for
    // None we need to build `AwsCredentialsProvider`.
    #[derivative(Default)]
    Default { load_timeout_secs: Option<u64> },
}

impl AwsAuthentication {
    pub async fn credentials_provider(
        &self,
        region: Region,
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
            AwsAuthentication::Role {
                assume_role,
                load_timeout_secs,
            } => {
                let provider = AssumeRoleProviderBuilder::new(assume_role)
                    .region(region.clone())
                    .build(default_credentials_provider(region, *load_timeout_secs).await);

                Ok(SharedCredentialsProvider::new(provider))
            }
            AwsAuthentication::Default { load_timeout_secs } => Ok(SharedCredentialsProvider::new(
                default_credentials_provider(region, *load_timeout_secs).await,
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

async fn default_credentials_provider(
    region: Region,
    load_timeout_secs: Option<u64>,
) -> SharedCredentialsProvider {
    let chain = DefaultCredentialsChain::builder()
        .region(region)
        .load_timeout(
            load_timeout_secs
                .map(Duration::from_secs)
                .unwrap_or(DEFAULT_LOAD_TIMEOUT),
        );

    SharedCredentialsProvider::new(chain.build().await)
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

        assert!(matches!(config.auth, AwsAuthentication::Default { .. }));
    }

    #[test]
    fn parsing_default_with_load_timeout() {
        let config = toml::from_str::<ComponentConfig>(
            r#"
            auth.load_timeout_secs = 10
        "#,
        )
        .unwrap();

        assert!(matches!(
            config.auth,
            AwsAuthentication::Default {
                load_timeout_secs: Some(10)
            }
        ));
    }

    #[test]
    fn parsing_old_assume_role() {
        let config = toml::from_str::<ComponentConfig>(
            r#"
            assume_role = "root"
        "#,
        )
        .unwrap();

        assert!(matches!(config.auth, AwsAuthentication::Default { .. }));
    }

    #[test]
    fn parsing_assume_role() {
        let config = toml::from_str::<ComponentConfig>(
            r#"
            auth.assume_role = "root"
            auth.load_timeout_secs = 10
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
            auth.load_timeout_secs = 10
        "#,
        )
        .unwrap();

        match config.auth {
            AwsAuthentication::Role {
                assume_role,
                load_timeout_secs,
            } => {
                assert_eq!(&assume_role, "auth.root");
                assert_eq!(load_timeout_secs, Some(10));
            }
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
