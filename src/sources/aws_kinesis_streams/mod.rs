pub mod batcher;
pub mod checkpointer;
pub mod closed_shard;
pub mod source;

use vector_lib::{
    codecs::decoding::{DeserializerConfig, FramingConfig},
    config::{LegacyKey, LogNamespace},
    configurable::configurable_component,
    lookup::owned_value_path,
};
use vrl::value::Kind;

use crate::{
    aws::{AwsAuthentication, create_client, region::RegionOrEndpoint},
    codecs::DecodingConfig,
    common::{dynamodb::DynamoDbClientBuilder, kinesis::KinesisClientBuilder},
    config::{SourceAcknowledgementsConfig, SourceConfig, SourceContext, SourceOutput},
    serde::{bool_or_struct, default_decoding, default_framing_message_based},
    tls::TlsConfig,
};

use self::source::KinesisStreamsSource;

/// Configuration for the DynamoDB-based checkpointing table.
#[configurable_component]
#[derive(Clone, Debug, Derivative)]
#[derivative(Default)]
#[serde(deny_unknown_fields)]
pub struct DynamoDbCheckpointConfig {
    /// The name of the DynamoDB table used for storing sequence checkpoints and
    /// coordinating distributed consumers.
    ///
    /// The table must have a String hash key named `StreamID` and a String range
    /// key named `ShardID`. If `create` is true, the table will be created if it
    /// does not exist.
    #[configurable(metadata(docs::examples = "vector-kinesis-checkpoints"))]
    pub table: String,

    /// If true, the DynamoDB table is created automatically if it does not
    /// already exist.
    #[serde(default)]
    pub create: bool,

    /// Billing mode to use when creating the table.
    ///
    /// Only relevant when `create` is true.
    #[serde(default = "default_billing_mode")]
    #[derivative(Default(value = "default_billing_mode()"))]
    pub billing_mode: String,

    /// Provisioned read capacity units for the table.
    ///
    /// Only used when `create` is true and `billing_mode` is `PROVISIONED`.
    #[serde(default)]
    pub read_capacity_units: i64,

    /// Provisioned write capacity units for the table.
    ///
    /// Only used when `create` is true and `billing_mode` is `PROVISIONED`.
    #[serde(default)]
    pub write_capacity_units: i64,

    /// How often (in seconds) to persist the latest consumed sequence number to DynamoDB.
    #[serde(default = "default_commit_period_secs")]
    #[derivative(Default(value = "default_commit_period_secs()"))]
    pub commit_period_secs: u64,
}

fn default_billing_mode() -> String {
    "PAY_PER_REQUEST".to_string()
}

/// Configuration for the `aws_kinesis_streams` source.
#[configurable_component(source(
    "aws_kinesis_streams",
    "Collect logs from AWS Kinesis Data Streams."
))]
#[derive(Clone, Debug, Derivative)]
#[derivative(Default)]
#[serde(deny_unknown_fields)]
pub struct AwsKinesisStreamsConfig {
    #[serde(flatten)]
    pub region: RegionOrEndpoint,

    #[configurable(derived)]
    #[serde(default)]
    pub auth: AwsAuthentication,

    /// One or more Kinesis data stream names or ARNs to consume from.
    ///
    /// Each entry can optionally include a shard suffix to consume only a specific
    /// shard, for example `my-stream:0` to consume shard 0. Multiple comma-separated
    /// streams may appear in a single list element.
    ///
    /// When explicit shards are listed, no DynamoDB coordination is performed; the
    /// source directly consumes those shards. When no shard suffix is given, shards
    /// are automatically balanced across all consumers using DynamoDB.
    #[configurable(metadata(docs::examples = "my-stream"))]
    #[configurable(metadata(docs::examples = "arn:aws:kinesis:us-east-1:123456789012:stream/my-stream"))]
    #[configurable(metadata(docs::examples = "my-stream:0"))]
    pub streams: Vec<String>,

    /// DynamoDB table configuration for storing sequence checkpoints and coordinating
    /// distributed consumers.
    pub dynamodb: DynamoDbCheckpointConfig,

    /// Maximum number of in-flight (unacknowledged) records per shard.
    ///
    /// Increasing this enables parallel downstream processing at the cost of higher
    /// memory usage. A value of 1 enforces strictly ordered, sequential processing.
    #[serde(default = "default_checkpoint_limit")]
    #[derivative(Default(value = "default_checkpoint_limit()"))]
    pub checkpoint_limit: u32,

    /// How long (in seconds) before a consumer that has not updated its checkpoint
    /// is considered inactive and its shards become eligible for re-claiming.
    #[serde(default = "default_lease_period_secs")]
    #[derivative(Default(value = "default_lease_period_secs()"))]
    pub lease_period_secs: u64,

    /// How often (in seconds) the source attempts to rebalance shards across
    /// all active consumers.
    #[serde(default = "default_rebalance_period_secs")]
    #[derivative(Default(value = "default_rebalance_period_secs()"))]
    pub rebalance_period_secs: u64,

    /// If true, consumption starts from the oldest available record in a shard when
    /// no checkpoint exists. If false, consumption starts from the newest record.
    #[serde(default = "default_start_from_oldest")]
    #[derivative(Default(value = "default_start_from_oldest()"))]
    pub start_from_oldest: bool,

    /// Maximum number of records to request per `GetRecords` call (1–10,000).
    ///
    /// Lower values keep each response well within the 2 MiB/sec per-shard read budget,
    /// reducing the risk of `ProvisionedThroughputExceededException`.  Higher values
    /// may improve throughput on low-volume shards at the cost of larger bursts.
    /// Values above 10,000 are silently capped to 10,000 by the source.
    #[serde(default = "default_max_records_per_call")]
    #[derivative(Default(value = "default_max_records_per_call()"))]
    pub max_records_per_call: i32,

    /// Minimum milliseconds to wait between consecutive `GetRecords` calls on a single shard.
    ///
    /// AWS enforces a hard limit of 5 `GetRecords` calls per second per shard. The default
    /// of 1000 ms matches the KCL `idleTimeBetweenReadsInMillis` default and leaves
    /// headroom for other consumers sharing the same shard. Accepted range: 200–1000.
    /// Values below 200 are clamped to 200 (the minimum safe interval); values above
    /// 1000 are clamped to 1000.
    #[serde(default = "default_poll_interval_ms")]
    #[derivative(Default(value = "default_poll_interval_ms()"))]
    pub poll_interval_ms: u64,

    #[configurable(derived)]
    #[serde(default = "default_framing_message_based")]
    #[derivative(Default(value = "default_framing_message_based()"))]
    pub framing: FramingConfig,

    #[configurable(derived)]
    #[serde(default = "default_decoding")]
    #[derivative(Default(value = "default_decoding()"))]
    pub decoding: DeserializerConfig,

    #[configurable(derived)]
    #[serde(default, deserialize_with = "bool_or_struct")]
    pub acknowledgements: SourceAcknowledgementsConfig,

    #[configurable(derived)]
    pub tls: Option<TlsConfig>,

    /// Overrides the global log namespace setting for this source.
    #[configurable(metadata(docs::hidden))]
    #[serde(default)]
    pub log_namespace: Option<bool>,
}

const fn default_checkpoint_limit() -> u32 {
    1024
}

const fn default_commit_period_secs() -> u64 {
    60
}

const fn default_lease_period_secs() -> u64 {
    30
}

const fn default_rebalance_period_secs() -> u64 {
    30
}

const fn default_start_from_oldest() -> bool {
    true
}

const fn default_max_records_per_call() -> i32 {
    1_000
}

const fn default_poll_interval_ms() -> u64 {
    1_000
}

impl_generate_config_from_default!(AwsKinesisStreamsConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "aws_kinesis_streams")]
impl SourceConfig for AwsKinesisStreamsConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<crate::sources::Source> {
        let log_namespace = cx.log_namespace(self.log_namespace);
        let acknowledgements = cx.do_acknowledgements(self.acknowledgements);

        let kinesis_client = create_client::<KinesisClientBuilder>(
            &KinesisClientBuilder {},
            &self.auth,
            self.region.region(),
            self.region.endpoint(),
            &cx.proxy,
            self.tls.as_ref(),
            None,
        )
        .await?;

        let dynamodb_client = create_client::<DynamoDbClientBuilder>(
            &DynamoDbClientBuilder {},
            &self.auth,
            self.region.region(),
            self.region.endpoint(),
            &cx.proxy,
            self.tls.as_ref(),
            None,
        )
        .await?;

        let decoder =
            DecodingConfig::new(self.framing.clone(), self.decoding.clone(), log_namespace)
                .build()?;

        let source = KinesisStreamsSource::new(
            self.clone(),
            kinesis_client,
            dynamodb_client,
            decoder,
            acknowledgements,
            log_namespace,
        )?;

        Ok(Box::pin(source.run(cx.out, cx.shutdown)))
    }

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        let log_namespace = global_log_namespace.merge(self.log_namespace);
        let schema_definition = self
            .decoding
            .schema_definition(log_namespace)
            .with_standard_vector_source_metadata()
            .with_source_metadata(
                Self::NAME,
                Some(LegacyKey::Overwrite(owned_value_path!("timestamp"))),
                &owned_value_path!("timestamp"),
                Kind::timestamp().or_undefined(),
                Some("timestamp"),
            );

        vec![SourceOutput::new_maybe_logs(
            self.decoding.output_type(),
            schema_definition,
        )]
    }

    fn can_acknowledge(&self) -> bool {
        true
    }
}
