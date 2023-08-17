#[cfg(feature = "aws-core")]
use aws_credential_types::provider::SharedCredentialsProvider;
#[cfg(feature = "aws-core")]
use aws_types::region::Region;

#[derive(Debug, Clone)]
pub enum Auth {
    Basic(crate::http::Auth),
    #[cfg(feature = "aws-core")]
    Aws {
        credentials_provider: SharedCredentialsProvider,
        region: Region,
    },
}
