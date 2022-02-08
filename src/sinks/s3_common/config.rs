use std::{collections::BTreeMap, convert::TryInto, time::Duration};

use futures::FutureExt;
use http::StatusCode;
use hyper::client;
use rusoto_core::RusotoError;
use rusoto_s3::{HeadBucketRequest, PutObjectError, S3Client, S3};
use serde::{Deserialize, Serialize};
use snafu::Snafu;

use super::service::{S3Response, S3Service};
use crate::{
    aws::{
        rusoto,
        rusoto::{AwsAuthentication, RegionOrEndpoint},
    },
    config::ProxyConfig,
    sinks::{util::retries::RetryLogic, Healthcheck},
};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct S3Options {
    pub acl: Option<S3CannedAcl>,
    pub grant_full_control: Option<String>,
    pub grant_read: Option<String>,
    pub grant_read_acp: Option<String>,
    pub grant_write_acp: Option<String>,
    pub server_side_encryption: Option<S3ServerSideEncryption>,
    pub ssekms_key_id: Option<String>,
    pub storage_class: Option<S3StorageClass>,
    pub tags: Option<BTreeMap<String, String>>,
    pub content_encoding: Option<String>, // inherit from compression value
    pub content_type: Option<String>,     // default `text/x-log`
}

#[derive(Clone, Copy, Debug, Derivative, Deserialize, PartialEq, Serialize)]
#[derivative(Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum S3StorageClass {
    #[derivative(Default)]
    Standard,
    ReducedRedundancy,
    IntelligentTiering,
    StandardIa,
    OnezoneIa,
    Glacier,
    DeepArchive,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub enum S3ServerSideEncryption {
    #[serde(rename = "AES256")]
    Aes256,
    #[serde(rename = "aws:kms")]
    AwsKms,
}

#[derive(Clone, Copy, Debug, Derivative, Deserialize, Serialize)]
#[derivative(Default)]
#[serde(rename_all = "kebab-case")]
pub enum S3CannedAcl {
    #[derivative(Default)]
    Private,
    PublicRead,
    PublicReadWrite,
    AwsExecRead,
    AuthenticatedRead,
    BucketOwnerRead,
    BucketOwnerFullControl,
    LogDeliveryWrite,
}

#[derive(Debug, Clone)]
pub struct S3RetryLogic;

impl RetryLogic for S3RetryLogic {
    type Error = RusotoError<PutObjectError>;
    type Response = S3Response;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        rusoto::is_retriable_error(error)
    }
}

#[derive(Debug, Snafu)]
pub enum HealthcheckError {
    #[snafu(display("Invalid credentials"))]
    InvalidCredentials,
    #[snafu(display("Unknown bucket: {:?}", bucket))]
    UnknownBucket { bucket: String },
    #[snafu(display("Unknown status code: {}", status))]
    UnknownStatus { status: StatusCode },
}

pub fn build_healthcheck(bucket: String, client: S3Client) -> crate::Result<Healthcheck> {
    let healthcheck = async move {
        let req = client
            .head_bucket(HeadBucketRequest {
                bucket: bucket.clone(),
                expected_bucket_owner: None,
            })
            .await;

        match req {
            Ok(_) => Ok(()),
            Err(error) => Err(match error {
                RusotoError::Unknown(resp) => match resp.status {
                    StatusCode::FORBIDDEN => HealthcheckError::InvalidCredentials.into(),
                    StatusCode::NOT_FOUND => HealthcheckError::UnknownBucket { bucket }.into(),
                    status => HealthcheckError::UnknownStatus { status }.into(),
                },
                error => error.into(),
            }),
        }
    };

    Ok(healthcheck.boxed())
}

pub fn create_service(
    region: &RegionOrEndpoint,
    auth: &AwsAuthentication,
    assume_role: Option<String>,
    proxy: &ProxyConfig,
) -> crate::Result<S3Service> {
    let region = region.try_into()?;
    let client = rusoto::custom_client(
        proxy,
        // S3 closes idle connections after 20 seconds,
        // so we can close idle connections ahead of time to prevent re-using them
        client::Client::builder().pool_idle_timeout(Duration::from_secs(15)),
    )?;

    let creds = auth.build(&region, assume_role)?;

    let client = S3Client::new_with(client, creds, region.clone());
    Ok(S3Service::new(client, region))
}

#[cfg(test)]
mod tests {
    use super::S3StorageClass;
    use crate::serde::to_string;

    #[test]
    fn storage_class_names() {
        for &(name, storage_class) in &[
            ("DEEP_ARCHIVE", S3StorageClass::DeepArchive),
            ("GLACIER", S3StorageClass::Glacier),
            ("INTELLIGENT_TIERING", S3StorageClass::IntelligentTiering),
            ("ONEZONE_IA", S3StorageClass::OnezoneIa),
            ("REDUCED_REDUNDANCY", S3StorageClass::ReducedRedundancy),
            ("STANDARD", S3StorageClass::Standard),
            ("STANDARD_IA", S3StorageClass::StandardIa),
        ] {
            assert_eq!(name, to_string(storage_class));
            let result: S3StorageClass = serde_json::from_str(&format!("{:?}", name))
                .unwrap_or_else(|error| {
                    panic!("Unparsable storage class name {:?}: {}", name, error)
                });
            assert_eq!(result, storage_class);
        }
    }
}
