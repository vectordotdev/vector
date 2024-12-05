use std::collections::{BTreeMap, HashMap};

use aws_sdk_s3::{
    operation::put_object::PutObjectError,
    types::{ObjectCannedAcl, ServerSideEncryption, StorageClass},
    Client as S3Client,
};
use aws_smithy_runtime_api::{
    client::{orchestrator::HttpResponse, result::SdkError},
    http::StatusCode,
};
use futures::FutureExt;
use snafu::Snafu;
use vector_lib::configurable::configurable_component;

use super::service::{S3Response, S3Service};
use crate::{
    aws::{create_client, is_retriable_error, AwsAuthentication, RegionOrEndpoint},
    common::s3::S3ClientBuilder,
    config::ProxyConfig,
    http::status,
    sinks::{util::retries::RetryLogic, Healthcheck},
    tls::TlsConfig,
};

/// Per-operation configuration when writing objects to S3.
#[configurable_component]
#[derive(Clone, Debug, Default)]
pub struct S3Options {
    /// Canned ACL to apply to the created objects.
    ///
    /// For more information, see [Canned ACL][canned_acl].
    ///
    /// [canned_acl]: https://docs.aws.amazon.com/AmazonS3/latest/dev/acl-overview.html#canned-acl
    pub acl: Option<S3CannedAcl>,

    /// Grants `READ`, `READ_ACP`, and `WRITE_ACP` permissions on the created objects to the named [grantee].
    ///
    /// This allows the grantee to read the created objects and their metadata, as well as read and
    /// modify the ACL on the created objects.
    ///
    /// [grantee]: https://docs.aws.amazon.com/AmazonS3/latest/dev/acl-overview.html#specifying-grantee
    #[configurable(metadata(
        docs::examples = "79a59df900b949e55d96a1e698fbacedfd6e09d98eacf8f8d5218e7cd47ef2be"
    ))]
    #[configurable(metadata(docs::examples = "person@email.com"))]
    #[configurable(metadata(docs::examples = "http://acs.amazonaws.com/groups/global/AllUsers"))]
    pub grant_full_control: Option<String>,

    /// Grants `READ` permissions on the created objects to the named [grantee].
    ///
    /// This allows the grantee to read the created objects and their metadata.
    ///
    /// [grantee]: https://docs.aws.amazon.com/AmazonS3/latest/dev/acl-overview.html#specifying-grantee
    #[configurable(metadata(
        docs::examples = "79a59df900b949e55d96a1e698fbacedfd6e09d98eacf8f8d5218e7cd47ef2be"
    ))]
    #[configurable(metadata(docs::examples = "person@email.com"))]
    #[configurable(metadata(docs::examples = "http://acs.amazonaws.com/groups/global/AllUsers"))]
    pub grant_read: Option<String>,

    /// Grants `READ_ACP` permissions on the created objects to the named [grantee].
    ///
    /// This allows the grantee to read the ACL on the created objects.
    ///
    /// [grantee]: https://docs.aws.amazon.com/AmazonS3/latest/dev/acl-overview.html#specifying-grantee
    #[configurable(metadata(
        docs::examples = "79a59df900b949e55d96a1e698fbacedfd6e09d98eacf8f8d5218e7cd47ef2be"
    ))]
    #[configurable(metadata(docs::examples = "person@email.com"))]
    #[configurable(metadata(docs::examples = "http://acs.amazonaws.com/groups/global/AllUsers"))]
    pub grant_read_acp: Option<String>,

    /// Grants `WRITE_ACP` permissions on the created objects to the named [grantee].
    ///
    /// This allows the grantee to modify the ACL on the created objects.
    ///
    /// [grantee]: https://docs.aws.amazon.com/AmazonS3/latest/dev/acl-overview.html#specifying-grantee
    #[configurable(metadata(
        docs::examples = "79a59df900b949e55d96a1e698fbacedfd6e09d98eacf8f8d5218e7cd47ef2be"
    ))]
    #[configurable(metadata(docs::examples = "person@email.com"))]
    #[configurable(metadata(docs::examples = "http://acs.amazonaws.com/groups/global/AllUsers"))]
    pub grant_write_acp: Option<String>,

    /// The Server-side Encryption algorithm used when storing these objects.
    pub server_side_encryption: Option<S3ServerSideEncryption>,

    /// Specifies the ID of the AWS Key Management Service (AWS KMS) symmetrical customer managed
    /// customer master key (CMK) that is used for the created objects.
    ///
    /// Only applies when `server_side_encryption` is configured to use KMS.
    ///
    /// If not specified, Amazon S3 uses the AWS managed CMK in AWS to protect the data.
    #[configurable(metadata(docs::examples = "abcd1234"))]
    #[configurable(metadata(docs::templateable))]
    pub ssekms_key_id: Option<String>,

    /// The storage class for the created objects.
    ///
    /// See the [S3 Storage Classes][s3_storage_classes] for more details.
    ///
    /// [s3_storage_classes]: https://docs.aws.amazon.com/AmazonS3/latest/dev/storage-class-intro.html
    #[serde(default)]
    pub storage_class: S3StorageClass,

    /// The tag-set for the object.
    #[configurable(metadata(docs::additional_props_description = "A single tag."))]
    #[configurable(metadata(docs::examples = "example_tags()"))]
    pub tags: Option<BTreeMap<String, String>>,

    /// Overrides what content encoding has been applied to the object.
    ///
    /// Directly comparable to the `Content-Encoding` HTTP header.
    ///
    /// If not specified, the compression scheme used dictates this value.
    #[configurable(metadata(docs::examples = "gzip"))]
    pub content_encoding: Option<String>,

    /// Overrides the MIME type of the object.
    ///
    /// Directly comparable to the `Content-Type` HTTP header.
    ///
    /// If not specified, the compression scheme used dictates this value.
    /// When `compression` is set to `none`, the value `text/x-log` is used.
    #[configurable(metadata(docs::examples = "application/gzip"))]
    pub content_type: Option<String>,
}

fn example_tags() -> HashMap<String, String> {
    HashMap::<_, _>::from_iter([
        ("Project".to_string(), "Blue".to_string()),
        ("Classification".to_string(), "confidential".to_string()),
        ("PHI".to_string(), "True".to_string()),
    ])
}

/// S3 storage classes.
///
/// More information on each storage class can be found in the [AWS documentation][aws_docs].
///
/// [aws_docs]: https://docs.aws.amazon.com/AmazonS3/latest/userguide/storage-class-intro.html
#[configurable_component]
#[derive(Clone, Copy, Debug, Derivative, PartialEq, Eq)]
#[derivative(Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum S3StorageClass {
    /// Standard Redundancy.
    #[derivative(Default)]
    Standard,

    /// Reduced Redundancy.
    ReducedRedundancy,

    /// Intelligent Tiering.
    IntelligentTiering,

    /// Infrequently Accessed.
    StandardIa,

    /// High Performance (single Availability zone).
    ExpressOnezone,

    /// Infrequently Accessed (single Availability zone).
    OnezoneIa,

    /// Glacier Flexible Retrieval.
    Glacier,

    /// Glacier Deep Archive.
    DeepArchive,
}

impl From<S3StorageClass> for StorageClass {
    fn from(x: S3StorageClass) -> Self {
        match x {
            S3StorageClass::Standard => Self::Standard,
            S3StorageClass::ReducedRedundancy => Self::ReducedRedundancy,
            S3StorageClass::IntelligentTiering => Self::IntelligentTiering,
            S3StorageClass::StandardIa => Self::StandardIa,
            S3StorageClass::ExpressOnezone => Self::ExpressOnezone,
            S3StorageClass::OnezoneIa => Self::OnezoneIa,
            S3StorageClass::Glacier => Self::Glacier,
            S3StorageClass::DeepArchive => Self::DeepArchive,
        }
    }
}

/// AWS S3 Server-Side Encryption algorithms.
///
/// More information on each algorithm can be found in the [AWS documentation][aws_docs].
///
/// [aws_docs]: https://docs.aws.amazon.com/AmazonS3/latest/userguide/serv-side-encryption.html
#[configurable_component]
#[derive(Clone, Copy, Debug)]
pub enum S3ServerSideEncryption {
    /// Each object is encrypted with AES-256 using a unique key.
    ///
    /// This corresponds to the `SSE-S3` option.
    #[serde(rename = "AES256")]
    Aes256,

    /// Each object is encrypted with AES-256 using keys managed by AWS KMS.
    ///
    /// Depending on whether or not a KMS key ID is specified, this corresponds either to the
    /// `SSE-KMS` option (keys generated/managed by KMS) or the `SSE-C` option (keys generated by
    /// the customer, managed by KMS).
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

/// S3 Canned ACLs.
///
/// For more information, see [Canned ACL][canned_acl].
///
/// [canned_acl]: https://docs.aws.amazon.com/AmazonS3/latest/dev/acl-overview.html#canned-acl
#[configurable_component]
#[derive(Clone, Copy, Debug, Derivative)]
#[derivative(Default)]
#[serde(rename_all = "kebab-case")]
pub enum S3CannedAcl {
    /// Bucket/object are private.
    ///
    /// The bucket/object owner is granted the `FULL_CONTROL` permission, and no one else has
    /// access.
    ///
    /// This is the default.
    #[derivative(Default)]
    Private,

    /// Bucket/object can be read publicly.
    ///
    /// The bucket/object owner is granted the `FULL_CONTROL` permission, and anyone in the
    /// `AllUsers` grantee group is granted the `READ` permission.
    PublicRead,

    /// Bucket/object can be read and written publicly.
    ///
    /// The bucket/object owner is granted the `FULL_CONTROL` permission, and anyone in the
    /// `AllUsers` grantee group is granted the `READ` and `WRITE` permissions.
    ///
    /// This is generally not recommended.
    PublicReadWrite,

    /// Bucket/object are private, and readable by EC2.
    ///
    /// The bucket/object owner is granted the `FULL_CONTROL` permission, and the AWS EC2 service is
    /// granted the `READ` permission for the purpose of reading Amazon Machine Image (AMI) bundles
    /// from the given bucket.
    AwsExecRead,

    /// Bucket/object can be read by authenticated users.
    ///
    /// The bucket/object owner is granted the `FULL_CONTROL` permission, and anyone in the
    /// `AuthenticatedUsers` grantee group is granted the `READ` permission.
    AuthenticatedRead,

    /// Object is private, except to the bucket owner.
    ///
    /// The object owner is granted the `FULL_CONTROL` permission, and the bucket owner is granted the `READ` permission.
    ///
    /// Only relevant when specified for an object: this canned ACL is otherwise ignored when
    /// specified for a bucket.
    BucketOwnerRead,

    /// Object is semi-private.
    ///
    /// Both the object owner and bucket owner are granted the `FULL_CONTROL` permission.
    ///
    /// Only relevant when specified for an object: this canned ACL is otherwise ignored when
    /// specified for a bucket.
    BucketOwnerFullControl,

    /// Bucket can have logs written.
    ///
    /// The `LogDelivery` grantee group is granted `WRITE` and `READ_ACP` permissions.
    ///
    /// Only relevant when specified for a bucket: this canned ACL is otherwise ignored when
    /// specified for an object.
    ///
    /// For more information about logs, see [Amazon S3 Server Access Logging][serverlogs].
    ///
    /// [serverlogs]: https://docs.aws.amazon.com/AmazonS3/latest/dev/ServerLogs.html
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
            S3CannedAcl::LogDeliveryWrite => ObjectCannedAcl::from("log-delivery-write"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct S3RetryLogic;

impl RetryLogic for S3RetryLogic {
    type Error = SdkError<PutObjectError, HttpResponse>;
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
                SdkError::ServiceError(inner) => {
                    let status = inner.into_raw().status();
                    match status.as_u16() {
                        status::FORBIDDEN => HealthcheckError::InvalidCredentials.into(),
                        status::NOT_FOUND => HealthcheckError::UnknownBucket { bucket }.into(),
                        _ => HealthcheckError::UnknownStatus { status }.into(),
                    }
                }
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
    tls_options: &Option<TlsConfig>,
) -> crate::Result<S3Service> {
    let endpoint = region.endpoint();
    let region = region.region();
    let client =
        create_client::<S3ClientBuilder>(auth, region.clone(), endpoint, proxy, tls_options, &None)
            .await?;
    Ok(S3Service::new(client))
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
            ("EXPRESS_ONEZONE", S3StorageClass::ExpressOnezone),
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
