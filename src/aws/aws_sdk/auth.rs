use aws_config::{default_provider::credentials::default_provider, sts::AssumeRoleProviderBuilder};
use aws_types::{credentials::SharedCredentialsProvider, Credentials};

use crate::aws::auth::AwsAuthentication;

impl AwsAuthentication {
    pub async fn credentials_provider(&self) -> crate::Result<SharedCredentialsProvider> {
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
            AwsAuthentication::Role { assume_role } => Ok(SharedCredentialsProvider::new(
                AssumeRoleProviderBuilder::new(assume_role)
                    .build(default_credentials_provider().await),
            )),
            AwsAuthentication::Default {} => Ok(SharedCredentialsProvider::new(
                default_credentials_provider().await,
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

async fn default_credentials_provider() -> SharedCredentialsProvider {
    SharedCredentialsProvider::new(default_provider().await)
}
