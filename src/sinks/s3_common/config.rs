use std::collections::BTreeMap;

use aws_sdk_s3::error::PutObjectError;
use aws_sdk_s3::model::{ObjectCannedAcl, ServerSideEncryption, StorageClass};
use aws_sdk_s3::Client as S3Client;
use aws_smithy_client::SdkError;
use futures::FutureExt;
use http::StatusCode;
use serde::{Deserialize, Serialize};
use snafu::Snafu;

use super::service::{S3Response, S3Service};
use crate::aws::{create_client, is_retriable_error};
use crate::aws::{AwsAuthentication, RegionOrEndpoint};
use crate::common::s3::S3ClientBuilder;
use crate::tls::TlsOptions;
use crate::{
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

impl From<S3StorageClass> for StorageClass {
    fn from(x: S3StorageClass) -> Self {
        match x {
            S3StorageClass::Standard => Self::Standard,
            S3StorageClass::ReducedRedundancy => Self::ReducedRedundancy,
            S3StorageClass::IntelligentTiering => Self::IntelligentTiering,
            S3StorageClass::StandardIa => Self::StandardIa,
            S3StorageClass::OnezoneIa => Self::OnezoneIa,
            S3StorageClass::Glacier => Self::Glacier,
            S3StorageClass::DeepArchive => Self::DeepArchive,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub enum S3ServerSideEncryption {
    #[serde(rename = "AES256")]
    Aes256,
    #[serde(rename = "aws:kms")]
    AwsKms,
}

impl From<S3ServerSideEncryption> for ServerSideEncryption {
    fn from(x: S3ServerSideEncryption) -> Self {
        match x {
            S3ServerSideEncryption::Aes256 => Self::Aes256,
            S3ServerSideEncryption::AwsKms => Self::AwsKms,
        }
    }
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

impl From<S3CannedAcl> for ObjectCannedAcl {
    fn from(x: S3CannedAcl) -> Self {
        match x {
            S3CannedAcl::Private => ObjectCannedAcl::Private,
            S3CannedAcl::PublicRead => ObjectCannedAcl::PublicRead,
            S3CannedAcl::PublicReadWrite => ObjectCannedAcl::PublicReadWrite,
            S3CannedAcl::AwsExecRead => ObjectCannedAcl::AwsExecRead,
            S3CannedAcl::AuthenticatedRead => ObjectCannedAcl::AuthenticatedRead,
            S3CannedAcl::BucketOwnerRead => ObjectCannedAcl::BucketOwnerRead,
            S3CannedAcl::BucketOwnerFullControl => ObjectCannedAcl::BucketOwnerFullControl,
            S3CannedAcl::LogDeliveryWrite => {
                ObjectCannedAcl::Unknown("log-delivery-write".to_string())
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct S3RetryLogic;

impl RetryLogic for S3RetryLogic {
    type Error = SdkError<PutObjectError>;
    type Response = S3Response;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        is_retriable_error(error)
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
            .head_bucket()
            .bucket(bucket.clone())
            .set_expected_bucket_owner(None)
            .send()
            .await;

        match req {
            Ok(_) => Ok(()),
            Err(error) => Err(match error {
                SdkError::ServiceError { err: _, raw } => match raw.http().status() {
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

pub async fn create_service(
    region: &RegionOrEndpoint,
    auth: &AwsAuthentication,
    proxy: &ProxyConfig,
    tls_options: &Option<TlsOptions>,
) -> crate::Result<S3Service> {
    let endpoint = region.endpoint()?;
    let region = region.region();
    let client =
        create_client::<S3ClientBuilder>(auth, region.clone(), endpoint, proxy, tls_options)
            .await?;
    Ok(S3Service::new(client, region))
}

#[cfg(test)]
mod tests {
    use super::S3StorageClass;
    use crate::serde::json::to_string;

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
