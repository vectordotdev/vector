use aws_sdk_cloudwatch::config::SharedInterceptor;

use crate::aws::ClientBuilder;

pub(crate) struct SqsClientBuilder;

impl ClientBuilder for SqsClientBuilder {
    type Config = aws_sdk_sqs::config::Config;
    type Client = aws_sdk_sqs::client::Client;
    // type DefaultMiddleware = aws_sdk_sqs::middleware::DefaultMiddleware;

    fn default_middleware() -> Vec<SharedInterceptor> {
        // aws_sdk_sqs::middleware::DefaultMiddleware::new()
        vec![]
    }

    fn build(config: &aws_types::SdkConfig) -> Self::Client {
        aws_sdk_sqs::client::Client::new(config)
    }
}
