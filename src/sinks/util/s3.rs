use vector_lib::configurable::configurable_component;

/// S3 storage classes.
///
/// More information on each storage class can be found in the [AWS documentation][aws_docs].
///
/// [aws_docs]: https://docs.aws.amazon.com/AmazonS3/latest/userguide/storage-class-intro.html
#[configurable_component]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum S3StorageClass {
    /// Standard Redundancy.
    #[default]
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

    /// Glacier Instant Retrieval.
    GlacierIr,

    /// Glacier Deep Archive.
    DeepArchive,
}
