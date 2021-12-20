use aws_config::{
    default_provider::credentials::default_provider,
    meta::credentials::LazyCachingCredentialsProvider, profile::ProfileFileCredentialsProvider,
    sts::AssumeRoleProviderBuilder,
};
use aws_types::{credentials::SharedCredentialsProvider, Credentials};

use crate::aws::auth::AwsAuthentication;

impl AwsAuthentication {
    pub async fn credentials_provider(&self) -> SharedCredentialsProvider {
        match self {
            Self::Static {
                access_key_id,
                secret_access_key,
            } => SharedCredentialsProvider::new(Credentials::from_keys(
                access_key_id,
                secret_access_key,
                None,
            )),
            AwsAuthentication::File {
                credentials_file,
                profile,
            } => {
                warn!("Overriding the credentials file is not supported. `~/.aws/config` and `~/.aws/credentials` will be used instead of \"{}\"", credentials_file);
                let mut file_provider = ProfileFileCredentialsProvider::builder();
                if let Some(profile) = profile {
                    file_provider = file_provider.profile_name(profile);
                }
                SharedCredentialsProvider::new(
                    LazyCachingCredentialsProvider::builder()
                        .load(file_provider.build())
                        .build(),
                )
            }
            AwsAuthentication::Role { assume_role } => SharedCredentialsProvider::new(
                AssumeRoleProviderBuilder::new(assume_role)
                    .build(default_credentials_provider().await),
            ),
            AwsAuthentication::Default {} => {
                SharedCredentialsProvider::new(default_credentials_provider().await)
            }
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
