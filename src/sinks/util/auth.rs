#[derive(Debug, Clone)]
pub enum Auth {
    Basic(crate::http::Auth),
    #[cfg(feature = "aws-core")]
    Aws {
        credentials_provider: Option<aws_credential_types::provider::SharedCredentialsProvider>,
        region: aws_types::region::Region,
    },
}
