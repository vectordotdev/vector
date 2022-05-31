use crate::aws::ClientBuilder;

pub(crate) struct SqsClientBuilder;

impl ClientBuilder for SqsClientBuilder {
    type Config = aws_sdk_sqs::config::Config;
    type Client = aws_sdk_sqs::client::Client;
    type DefaultMiddleware = aws_sdk_sqs::middleware::DefaultMiddleware;

    fn default_middleware() -> Self::DefaultMiddleware {
        aws_sdk_sqs::middleware::DefaultMiddleware::new()
    }

    fn build(client: aws_smithy_client::Client, config: &aws_types::SdkConfig) -> Self::Client {
        aws_sdk_sqs::client::Client::with_config(client, config.into())
    }
}
