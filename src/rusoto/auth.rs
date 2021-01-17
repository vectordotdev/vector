use super::AwsCredentialsProvider;
use rusoto_core::Region;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum AWSAuthentication {
    Role { assume_role: String },
    Default,
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
            Self::Role { assume_role } => {
                if old_assume_role.is_some() {
                    warn!("Ignoring option `assume_role` and using option `auth.assume_role` instead.");
                }
                AwsCredentialsProvider::new(region, Some(assume_role.clone()))
            }
            Self::Default => AwsCredentialsProvider::new(region, old_assume_role),
        }
    }
}

impl Default for AWSAuthentication {
    fn default() -> Self {
        Self::Default
    }
}
