use crate::aws::ClientBuilder;

pub(crate) struct S3ClientBuilder;

impl ClientBuilder for S3ClientBuilder {
    type Config = aws_sdk_s3::config::Config;
    type Client = aws_sdk_s3::client::Client;
    type DefaultMiddleware = aws_sdk_s3::middleware::DefaultMiddleware;

    fn default_middleware() -> Self::DefaultMiddleware {
        aws_sdk_s3::middleware::DefaultMiddleware::new()
    }

    fn build(client: aws_smithy_client::Client, config: &aws_types::SdkConfig) -> Self::Client {
        aws_sdk_s3::client::Client::with_config(client, config.into())
    }
}
