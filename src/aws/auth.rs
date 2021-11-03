use crate::aws::rusoto::AwsCredentialsProvider;
use rusoto_core::Region;
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



#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile;

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
