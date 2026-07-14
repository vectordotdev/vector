use aws_sdk_s3::config::{self, RequestChecksumCalculation};

use crate::aws::ClientBuilder;

pub(crate) struct S3ClientBuilder {
    pub force_path_style: Option<bool>,
}

impl ClientBuilder for S3ClientBuilder {
    type Client = aws_sdk_s3::client::Client;

    fn build(&self, config: &aws_types::SdkConfig) -> Self::Client {
        let builder = config::Builder::from(config)
            .force_path_style(self.force_path_style.unwrap_or(true))
            // The AWS SDK defaults to always calculating a `x-amz-checksum-*` request
            // checksum on writes. Vector already computes and sends its own
            // `Content-MD5` header for `PutObject` requests (see
            // `s3_common::service::S3Service`), so the SDK's additional checksum is
            // redundant against AWS S3 and gets rejected outright by S3-compatible
            // providers such as Cloudflare R2 with
            // "InvalidRequest: You can only specify one non-default checksum at a time."
            // Restricting checksum calculation to only when required keeps AWS S3
            // behavior correct while fixing compatibility with those providers.
            // Response checksum validation is left at the SDK default: it only
            // affects reads (e.g. the `aws_s3` source's `GetObject` calls), which
            // aren't part of the R2 `PutObject` failure this fixes, so weakening it
            // here would drop integrity checking on downloads for no reason.
            // See https://github.com/vectordotdev/vector/issues/23029 and
            // https://github.com/awslabs/aws-sdk-rust/issues/1240.
            .request_checksum_calculation(RequestChecksumCalculation::WhenRequired);
        aws_sdk_s3::client::Client::from_conf(builder.build())
    }
}

#[cfg(test)]
mod tests {
    use aws_types::{SdkConfig, region::Region};

    use super::*;

    #[test]
    fn checksum_behavior_defaults_to_when_required() {
        let sdk_config = SdkConfig::builder()
            .region(Region::new("us-east-1"))
            .build();

        let client = S3ClientBuilder {
            force_path_style: None,
        }
        .build(&sdk_config);

        let config = client.config();

        assert_eq!(
            config.request_checksum_calculation(),
            Some(&RequestChecksumCalculation::WhenRequired),
            "request checksum calculation must be restricted to when required so Vector's own \
             Content-MD5 header isn't paired with an SDK-added x-amz-checksum-* header, which \
             S3-compatible providers such as Cloudflare R2 reject as an InvalidRequest",
        );
    }
}
