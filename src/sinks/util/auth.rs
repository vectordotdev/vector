use http::{HeaderValue, Request};
use vector_lib::sensitive_string::SensitiveString;

#[derive(Debug, Clone)]
pub enum Auth {
    Basic(crate::http::Auth),
    ApiKey(SensitiveString),
    #[cfg(feature = "aws-core")]
    Aws {
        credentials_provider: aws_credential_types::provider::SharedCredentialsProvider,
        region: aws_types::region::Region,
    },
}

pub fn apply_api_key<B>(api_key: &SensitiveString, request: &mut Request<B>) -> crate::Result<()> {
    let mut value = HeaderValue::from_str(&format!("ApiKey {}", api_key.inner()))?;
    value.set_sensitive(true);
    request
        .headers_mut()
        .insert(http::header::AUTHORIZATION, value);
    Ok(())
}
