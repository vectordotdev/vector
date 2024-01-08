use crate::aws::ClientBuilder;

pub(crate) struct SqsClientBuilder;

impl ClientBuilder for SqsClientBuilder {
    type Client = aws_sdk_sqs::client::Client;

    fn build(config: &aws_types::SdkConfig) -> Self::Client {
        aws_sdk_sqs::client::Client::new(config)
    }
}
