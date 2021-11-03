use rusoto_core::Region;
use crate::aws::auth::AwsAuthentication;
use crate::aws::rusoto::AwsCredentialsProvider;

const AWS_DEFAULT_PROFILE: &'static str = "default";

impl AwsAuthentication {


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
            Self::File {
                credentials_file,
                profile,
            } => {
                if old_assume_role.is_some() {
                    warn!(
                        "Ignoring option `assume_role`, instead using AWS credentials file options."
                    );
                }
                AwsCredentialsProvider::new_with_credentials_file(
                    credentials_file,
                    profile
                        .as_ref()
                        .unwrap_or(&AWS_DEFAULT_PROFILE.to_string())
                        .as_str(),
                )
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
