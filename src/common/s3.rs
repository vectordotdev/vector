use aws_sdk_s3::config;

use crate::aws::ClientBuilder;

pub(crate) struct S3ClientBuilder;

impl ClientBuilder for S3ClientBuilder {
    type Client = aws_sdk_s3::client::Client;

    fn build(config: &aws_types::SdkConfig) -> Self::Client {
        let config = config::Builder::from(config).force_path_style(true).build();
        aws_sdk_s3::client::Client::from_conf(config)
    }
}
