use rusoto_core::Region;

use crate::aws::{auth::AwsAuthentication, rusoto::AwsCredentialsProvider};

const AWS_DEFAULT_PROFILE: &str = "default";

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
#[cfg(test)]
mod test {
    use std::{fs::File, io::Write};

    use rusoto_core::Region;

    use super::*;
    use crate::aws::auth::AwsAuthentication;

    #[test]
    fn parsing_credentials_file() {
        let tmpdir = tempfile::tempdir().unwrap();
        let tmpfile_path = tmpdir.path().join("credentials");
        let mut tmpfile = File::create(&tmpfile_path).unwrap();

        writeln!(
            tmpfile,
            r#"
            [default]
            aws_access_key_id = default-access-key-id
            aws_secret_access_key = default-secret
        "#
        )
        .unwrap();

        let auth = AwsAuthentication::File {
            credentials_file: tmpfile_path.to_str().unwrap().to_string(),
            profile: Some("default".to_string()),
        };
        let result = auth.build(&Region::AfSouth1, None).unwrap();
        assert!(matches!(result, AwsCredentialsProvider::File { .. }));

        drop(tmpfile);
        tmpdir.close().unwrap();
    }
}
