use crate::aws::ClientBuilder;

pub(crate) struct DynamoDbClientBuilder;

impl ClientBuilder for DynamoDbClientBuilder {
    type Client = aws_sdk_dynamodb::Client;

    fn build(&self, config: &aws_types::SdkConfig) -> Self::Client {
        aws_sdk_dynamodb::Client::new(config)
    }
}
