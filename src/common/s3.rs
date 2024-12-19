use aws_sdk_s3::config;

use crate::aws::ClientBuilder;

pub(crate) struct S3ClientBuilder {
    pub force_path_style: Option<bool>,
}

impl ClientBuilder for S3ClientBuilder {
    type Client = aws_sdk_s3::client::Client;

    fn build(&self, config: &aws_types::SdkConfig) -> Self::Client {
        let mut builder = config::Builder::from(config);

        if let Some(true) = self.force_path_style {
            builder = builder.force_path_style(true);
        }

        aws_sdk_s3::client::Client::from_conf(builder.build())
    }
}
