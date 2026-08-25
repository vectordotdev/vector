use crate::aws::ClientBuilder;

pub(crate) struct KinesisClientBuilder;

impl ClientBuilder for KinesisClientBuilder {
    type Client = aws_sdk_kinesis::Client;

    fn build(&self, config: &aws_types::SdkConfig) -> Self::Client {
        aws_sdk_kinesis::Client::new(config)
    }
}
