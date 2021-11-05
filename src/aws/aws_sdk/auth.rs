use crate::aws::auth::AwsAuthentication;
use aws_config::default_provider::credentials::default_provider;
use aws_config::meta::credentials::LazyCachingCredentialsProvider;
use aws_config::profile::ProfileFileCredentialsProvider;
use aws_config::provider_config::ProviderConfig;
use aws_config::sts::AssumeRoleProviderBuilder;
use aws_types::config::{Builder, Config};
use aws_types::credentials::SharedCredentialsProvider;
use aws_types::region::Region;
use aws_types::Credentials;

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

    // pub async fn build_config(&self, region: Region) -> Config {
    //     let mut builder = Builder::default().region(region);
    //
    //     let credentials_provider = ;
    //     builder.credentials_provider(credentials_provider).build()
    // }

    pub(crate) fn test_auth() -> AwsAuthentication {
        AwsAuthentication::Static {
            access_key_id: "dummy".to_string(),
            secret_access_key: "dummy".to_string(),
        }
    }
}

async fn default_credentials_provider() -> SharedCredentialsProvider {
    SharedCredentialsProvider::new(default_provider().await)
}
