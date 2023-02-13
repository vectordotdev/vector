use std::time::Duration;

use aws_config::{
    default_provider::credentials::DefaultCredentialsChain, imds, sts::AssumeRoleProviderBuilder,
};
use aws_credential_types::provider::SharedCredentialsProvider;
use aws_credential_types::Credentials;
use aws_types::region::Region;
use serde_with::serde_as;
use vector_common::sensitive_string::SensitiveString;
use vector_config::configurable_component;

// matches default load timeout from the SDK as of 0.10.1, but lets us confidently document the
// default rather than relying on the SDK default to not change
const DEFAULT_LOAD_TIMEOUT: Duration = Duration::from_secs(5);

/// IMDS Client Configuration for authenticating with AWS.
#[serde_as]
#[configurable_component]
#[derive(Copy, Clone, Debug, Derivative)]
#[derivative(Default)]
#[serde(deny_unknown_fields)]
pub struct ImdsAuthentication {
    /// Number of IMDS retries for fetching tokens and metadata.
    #[serde(default = "default_max_attempts")]
    #[derivative(Default(value = "default_max_attempts()"))]
    max_attempts: u32,

    /// Connect timeout for IMDS.
    #[serde(default = "default_timeout")]
    #[serde(rename = "connect_timeout_seconds")]
    #[serde_as(as = "serde_with::DurationSeconds<u64>")]
    #[derivative(Default(value = "default_timeout()"))]
    connect_timeout: Duration,

    /// Read timeout for IMDS.
    #[serde(default = "default_timeout")]
    #[serde(rename = "read_timeout_seconds")]
    #[serde_as(as = "serde_with::DurationSeconds<u64>")]
    #[derivative(Default(value = "default_timeout()"))]
    read_timeout: Duration,
}

const fn default_max_attempts() -> u32 {
    4
}

const fn default_timeout() -> Duration {
    Duration::from_secs(1)
}

/// Configuration of the authentication strategy for interacting with AWS services.
#[configurable_component]
#[derive(Clone, Debug, Derivative)]
#[derivative(Default)]
#[serde(deny_unknown_fields, untagged)]
pub enum AwsAuthentication {
    /// Authenticate using a fixed access key and secret pair.
    AccessKey {
        /// The AWS access key ID.
        #[configurable(metadata(docs::examples = "AKIAIOSFODNN7EXAMPLE"))]
        access_key_id: SensitiveString,

        /// The AWS secret access key.
        #[configurable(metadata(docs::examples = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"))]
        secret_access_key: SensitiveString,
    },

    /// Authenticate using credentials stored in a file.
    ///
    /// Additionally, the specific credential profile to use can be set.
    File {
        /// Path to the credentials file.
        #[configurable(metadata(docs::examples = "/my/aws/credentials"))]
        credentials_file: String,

        /// The credentials profile to use.
        ///
        /// Used to select AWS credentials from a provided credentials file.
        #[configurable(metadata(docs::examples = "develop"))]
        profile: Option<String>,
    },

    /// Assume the given role ARN.
    Role {
        /// The ARN of an [IAM role][iam_role] to assume.
        ///
        /// [iam_role]: https://docs.aws.amazon.com/IAM/latest/UserGuide/id_roles.html
        #[configurable(metadata(docs::examples = "arn:aws:iam::123456789098:role/my_role"))]
        assume_role: String,

        /// Timeout for assuming the role, in seconds.
        ///
        /// Relevant when the default credentials chain is used or `assume_role`.
        #[configurable(metadata(docs::type_unit = "seconds"))]
        #[configurable(metadata(docs::examples = 30))]
        load_timeout_secs: Option<u64>,

        /// Configuration for authenticating with AWS through IMDS.
        #[serde(default)]
        imds: ImdsAuthentication,

        /// The [AWS region][aws_region] to send STS requests to.
        ///
        /// If not set, this will default to the configured region
        /// for the service itself.
        ///
        /// [aws_region]: https://docs.aws.amazon.com/general/latest/gr/rande.html#regional-endpoints
        #[configurable(metadata(docs::examples = "us-west-2"))]
        region: Option<String>,
    },

    /// Default authentication strategy which tries a variety of substrategies in a one-after-the-other fashion.
    #[derivative(Default)]
    Default {
        /// Timeout for successfully loading any credentials, in seconds.
        ///
        /// Relevant when the default credentials chain is used or `assume_role`.
        #[configurable(metadata(docs::type_unit = "seconds"))]
        #[configurable(metadata(docs::examples = 30))]
        load_timeout_secs: Option<u64>,

        /// Configuration for authenticating with AWS through IMDS.
        #[serde(default)]
        imds: ImdsAuthentication,
    },
}

impl AwsAuthentication {
    pub async fn credentials_provider(
        &self,
        service_region: Region,
    ) -> crate::Result<SharedCredentialsProvider> {
        match self {
            Self::AccessKey {
                access_key_id,
                secret_access_key,
            } => Ok(SharedCredentialsProvider::new(Credentials::from_keys(
                access_key_id.inner(),
                secret_access_key.inner(),
                None,
            ))),
            AwsAuthentication::File { .. } => {
                Err("Overriding the credentials file is not supported.".into())
            }
            AwsAuthentication::Role {
                assume_role,
                load_timeout_secs,
                imds,
                region,
            } => {
                let auth_region = region.clone().map(Region::new).unwrap_or(service_region);
                let provider = AssumeRoleProviderBuilder::new(assume_role)
                    .region(auth_region.clone())
                    .build(
                        default_credentials_provider(auth_region, *load_timeout_secs, *imds)
                            .await?,
                    );

                Ok(SharedCredentialsProvider::new(provider))
            }
            AwsAuthentication::Default {
                load_timeout_secs,
                imds,
            } => Ok(SharedCredentialsProvider::new(
                default_credentials_provider(service_region, *load_timeout_secs, *imds).await?,
            )),
        }
    }

    #[cfg(test)]
    pub fn test_auth() -> AwsAuthentication {
        AwsAuthentication::AccessKey {
            access_key_id: "dummy".to_string().into(),
            secret_access_key: "dummy".to_string().into(),
        }
    }
}

async fn default_credentials_provider(
    region: Region,
    load_timeout_secs: Option<u64>,
    imds: ImdsAuthentication,
) -> crate::Result<SharedCredentialsProvider> {
    let client = imds::Client::builder()
        .max_attempts(imds.max_attempts)
        .connect_timeout(imds.connect_timeout)
        .read_timeout(imds.read_timeout)
        .build()
        .await?;

    let chain = DefaultCredentialsChain::builder()
        .region(region)
        .imds_client(client)
        .load_timeout(
            load_timeout_secs
                .map(Duration::from_secs)
                .unwrap_or(DEFAULT_LOAD_TIMEOUT),
        );

    Ok(SharedCredentialsProvider::new(chain.build().await))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
    const READ_TIMEOUT: Duration = Duration::from_secs(10);

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
                load_timeout_secs: Some(10),
                imds: ImdsAuthentication { .. },
            }
        ));
    }

    #[test]
    fn parsing_default_with_imds_client() {
        let config = toml::from_str::<ComponentConfig>(
            r#"
            auth.imds.max_attempts = 5
            auth.imds.connect_timeout_seconds = 30
            auth.imds.read_timeout_seconds = 10
        "#,
        )
        .unwrap();

        assert!(matches!(
            config.auth,
            AwsAuthentication::Default {
                load_timeout_secs: None,
                imds: ImdsAuthentication {
                    max_attempts: 5,
                    connect_timeout: CONNECT_TIMEOUT,
                    read_timeout: READ_TIMEOUT,
                },
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
    fn parsing_assume_role_with_imds_client() {
        let config = toml::from_str::<ComponentConfig>(
            r#"
            auth.assume_role = "root"
            auth.imds.max_attempts = 5
            auth.imds.connect_timeout_seconds = 30
            auth.imds.read_timeout_seconds = 10
        "#,
        )
        .unwrap();

        match config.auth {
            AwsAuthentication::Role {
                assume_role,
                load_timeout_secs,
                imds,
                region,
            } => {
                assert_eq!(&assume_role, "root");
                assert_eq!(load_timeout_secs, None);
                assert!(matches!(
                    imds,
                    ImdsAuthentication {
                        max_attempts: 5,
                        connect_timeout: CONNECT_TIMEOUT,
                        read_timeout: READ_TIMEOUT,
                    }
                ));
                assert_eq!(region, None);
            }
            _ => panic!(),
        }
    }

    #[test]
    fn parsing_both_assume_role() {
        let config = toml::from_str::<ComponentConfig>(
            r#"
            assume_role = "root"
            auth.assume_role = "auth.root"
            auth.load_timeout_secs = 10
            auth.region = "us-west-2"
        "#,
        )
        .unwrap();

        match config.auth {
            AwsAuthentication::Role {
                assume_role,
                load_timeout_secs,
                imds,
                region,
            } => {
                assert_eq!(&assume_role, "auth.root");
                assert_eq!(load_timeout_secs, Some(10));
                assert!(matches!(imds, ImdsAuthentication { .. }));
                assert_eq!(region.unwrap(), "us-west-2");
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

        assert!(matches!(config.auth, AwsAuthentication::AccessKey { .. }));
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
