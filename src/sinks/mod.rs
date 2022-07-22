use futures::future::BoxFuture;
use snafu::Snafu;

pub mod util;

#[cfg(feature = "sinks-aws_cloudwatch_logs")]
pub mod aws_cloudwatch_logs;
#[cfg(feature = "sinks-aws_cloudwatch_metrics")]
pub mod aws_cloudwatch_metrics;
#[cfg(feature = "sinks-aws_kinesis_firehose")]
pub mod aws_kinesis_firehose;
#[cfg(feature = "sinks-aws_kinesis_streams")]
pub mod aws_kinesis_streams;
#[cfg(feature = "sinks-aws_s3")]
pub mod aws_s3;
#[cfg(feature = "sinks-aws_sqs")]
pub mod aws_sqs;
#[cfg(feature = "sinks-axiom")]
pub mod axiom;
#[cfg(feature = "sinks-azure_blob")]
pub mod azure_blob;
#[cfg(any(feature = "sinks-azure_blob", feature = "sinks-datadog_archives"))]
pub mod azure_common;
#[cfg(feature = "sinks-azure_monitor_logs")]
pub mod azure_monitor_logs;
#[cfg(feature = "sinks-blackhole")]
pub mod blackhole;
#[cfg(feature = "sinks-clickhouse")]
pub mod clickhouse;
#[cfg(feature = "sinks-console")]
pub mod console;
#[cfg(any(
    feature = "sinks-datadog_events",
    feature = "sinks-datadog_logs",
    feature = "sinks-datadog_metrics",
    feature = "sinks-datadog_traces"
))]
pub mod datadog;
#[cfg(feature = "sinks-datadog_archives")]
pub mod datadog_archives;
#[cfg(feature = "sinks-elasticsearch")]
pub mod elasticsearch;
#[cfg(feature = "sinks-file")]
pub mod file;
#[cfg(feature = "sinks-gcp")]
pub mod gcp;
#[cfg(any(feature = "sinks-gcp"))]
pub mod gcs_common;
#[cfg(feature = "sinks-honeycomb")]
pub mod honeycomb;
#[cfg(feature = "sinks-http")]
pub mod http;
#[cfg(feature = "sinks-humio")]
pub mod humio;
#[cfg(any(feature = "sinks-influxdb", feature = "prometheus-integration-tests"))]
pub mod influxdb;
#[cfg(feature = "sinks-kafka")]
pub mod kafka;
#[cfg(feature = "sinks-logdna")]
pub mod logdna;
#[cfg(feature = "sinks-loki")]
pub mod loki;
#[cfg(feature = "sinks-nats")]
pub mod nats;
#[cfg(feature = "sinks-new_relic")]
pub mod new_relic;
#[cfg(feature = "sinks-new_relic_logs")]
pub mod new_relic_logs;
#[cfg(feature = "sinks-papertrail")]
pub mod papertrail;
#[cfg(feature = "sinks-prometheus")]
pub mod prometheus;
#[cfg(feature = "sinks-pulsar")]
pub mod pulsar;
#[cfg(feature = "sinks-redis")]
pub mod redis;
#[cfg(all(
    any(feature = "sinks-aws_s3", feature = "sinks-datadog_archives"),
    feature = "aws-core"
))]
pub mod s3_common;
#[cfg(feature = "sinks-sematext")]
pub mod sematext;
#[cfg(feature = "sinks-socket")]
pub mod socket;
#[cfg(feature = "sinks-splunk_hec")]
pub mod splunk_hec;
#[cfg(feature = "sinks-statsd")]
pub mod statsd;
#[cfg(feature = "sinks-vector")]
pub mod vector;
#[cfg(feature = "sinks-websocket")]
pub mod websocket;

use vector_config::configurable_component;
pub use vector_core::sink::VectorSink;

pub type Healthcheck = BoxFuture<'static, crate::Result<()>>;

/// Common build errors
#[derive(Debug, Snafu)]
pub enum BuildError {
    #[snafu(display("Unable to resolve DNS for {:?}", address))]
    DnsFailure { address: String },
    #[snafu(display("DNS errored {}", source))]
    DnsError { source: crate::dns::DnsError },
    #[snafu(display("Socket address problem: {}", source))]
    SocketAddressError { source: std::io::Error },
    #[snafu(display("URI parse error: {}", source))]
    UriParseError { source: ::http::uri::InvalidUri },
}

/// Common healthcheck errors
#[derive(Debug, Snafu)]
pub enum HealthcheckError {
    #[snafu(display("Unexpected status: {}", status))]
    UnexpectedStatus { status: ::http::StatusCode },
}

/// Configurable sinks in Vector.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Sinks {
    /// AWS CloudWatch Logs.
    #[cfg(feature = "sinks-aws_cloudwatch_logs")]
    AwsCloudwatchLogs(#[configurable(derived)] aws_cloudwatch_logs::CloudwatchLogsSinkConfig),

    /// AWS CloudWatch Metrics.
    #[cfg(feature = "sinks-aws_cloudwatch_metrics")]
    AwsCloudwatchMetrics(
        #[configurable(derived)] aws_cloudwatch_metrics::CloudWatchMetricsSinkConfig,
    ),

    /// AWS Kinesis Firehose.
    #[cfg(feature = "sinks-aws_kinesis_firehose")]
    AwsKinesisFirehose(#[configurable(derived)] aws_kinesis_firehose::KinesisFirehoseSinkConfig),

    /// AWS Kinesis Streams.
    #[cfg(feature = "sinks-aws_kinesis_streams")]
    AwsKinesisStreams(#[configurable(derived)] aws_kinesis_streams::KinesisSinkConfig),

    /// AWS S3.
    #[cfg(feature = "sinks-aws_s3")]
    AwsS3(#[configurable(derived)] aws_s3::S3SinkConfig),

    /// AWS SQS.
    #[cfg(feature = "sinks-aws_sqs")]
    AwsSqs(#[configurable(derived)] aws_sqs::SqsSinkConfig),

    /// Azure Blob Storage.
    #[cfg(feature = "sinks-azure_blob")]
    AzureBlob(#[configurable(derived)] azure_blob::AzureBlobSinkConfig),

    /// Azure Monitor Logs.
    #[cfg(feature = "sinks-azure_monitor_logs")]
    AzureMonitorLogs(#[configurable(derived)] azure_monitor_logs::AzureMonitorLogsConfig),

    /// Blackhole.
    #[cfg(feature = "sinks-blackhole")]
    Blackhole(#[configurable(derived)] blackhole::BlackholeConfig),

    /// Clickhouse.
    #[cfg(feature = "sinks-clickhouse")]
    Clickhouse(#[configurable(derived)] clickhouse::ClickhouseConfig),

    /// Console.
    #[cfg(feature = "sinks-console")]
    Console(#[configurable(derived)] console::ConsoleSinkConfig),

    /// Datadog Archives.
    #[cfg(feature = "sinks-datadog_archives")]
    DatadogArchives(#[configurable(derived)] datadog_archives::DatadogArchivesSinkConfig),

    /// Datadog Events.
    #[cfg(feature = "sinks-datadog_events")]
    DatadogEvents(#[configurable(derived)] datadog::events::DatadogEventsConfig),

    /// Datadog Logs.
    #[cfg(feature = "sinks-datadog_logs")]
    DatadogLogs(#[configurable(derived)] datadog::logs::DatadogLogsConfig),

    /// Datadog Metrics.
    #[cfg(feature = "sinks-datadog_metrics")]
    DatadogMetrics(#[configurable(derived)] datadog::metrics::DatadogMetricsConfig),

    /// Datadog Traces.
    #[cfg(feature = "sinks-datadog_traces")]
    DatadogTraces(#[configurable(derived)] datadog::traces::DatadogTracesConfig),

    /// Elasticsearch.
    #[cfg(feature = "sinks-elasticsearch")]
    Elasticsearch(#[configurable(derived)] elasticsearch::ElasticsearchConfig),

    /// File.
    #[cfg(feature = "sinks-file")]
    File(#[configurable(derived)] file::FileSinkConfig),

    /// GCP Stackdriver Logs.
    #[cfg(feature = "sinks-gcp")]
    GcpStackdriverLogs(#[configurable(derived)] gcp::stackdriver_logs::StackdriverConfig),

    /// GCP Stackdriver Metrics.
    #[cfg(feature = "sinks-gcp")]
    GcpStackdriverMetrics(#[configurable(derived)] gcp::stackdriver_metrics::StackdriverConfig),

    /// GCP Cloud Storage.
    #[cfg(feature = "sinks-gcp")]
    GcpCloudStorage(#[configurable(derived)] gcp::cloud_storage::GcsSinkConfig),

    /// GCP Pub/Sub.
    #[cfg(feature = "sinks-gcp")]
    GcpPubsub(#[configurable(derived)] gcp::pubsub::PubsubConfig),

    /// Honeycomb.
    #[cfg(feature = "sinks-honeycomb")]
    Honeycomb(#[configurable(derived)] honeycomb::HoneycombConfig),

    /// HTTP.
    #[cfg(feature = "sinks-http")]
    Http(#[configurable(derived)] http::HttpSinkConfig),

    /// Humio Logs.
    #[cfg(feature = "sinks-humio")]
    HumioLogs(#[configurable(derived)] humio::logs::HumioLogsConfig),

    /// Humio Metrics.
    #[cfg(feature = "sinks-humio")]
    HumioMetrics(#[configurable(derived)] humio::metrics::HumioMetricsConfig),

    /// InfluxDB Logs.
    #[cfg(any(feature = "sinks-influxdb", feature = "prometheus-integration-tests"))]
    InfluxdbLogs(#[configurable(derived)] influxdb::logs::InfluxDbLogsConfig),

    /*
    /// InfluxDB Metrics.
    #[cfg(any(feature = "sinks-influxdb", feature = "prometheus-integration-tests"))]
    InfluxdbMetrics(#[configurable(derived)] influxdb::metrics::InfluxDbConfig),

    /// Kafka.
    #[cfg(feature = "sinks-kafka")]
    Kafka(#[configurable(derived)] kafka::config::KafkaSinkConfig),

    /// LogDNA.
    #[cfg(feature = "sinks-logdna")]
    Logdna(#[configurable(derived)] logdna::LogdnaConfig),

    /// Loki.
    #[cfg(feature = "sinks-loki")]
    Loki(#[configurable(derived)] loki::LokiConfig),

    /// NATS.
    #[cfg(feature = "sinks-nats")]
    Nats(#[configurable(derived)] self::nats::NatsSinkConfig),

    /// New Relic.
    #[cfg(feature = "sinks-new_relic")]
    NewRelic(#[configurable(derived)] new_relic::NewRelicConfig),

    /// New Relic Logs.
    #[cfg(feature = "sinks-new_relic_logs")]
    NewrelicLogs(#[configurable(derived)] new_relic_logs::NewRelicLogsConfig),
    */
    /// Papertrail.
    #[cfg(feature = "sinks-papertrail")]
    Papertrail(#[configurable(derived)] papertrail::PapertrailConfig),

    /*
    /// Prometheus Exporter.
    #[cfg(feature = "sinks-prometheus")]
    PrometheusExporter(#[configurable(derived)] prometheus::exporter::PrometheusExporterConfig),

    /// Prometheus Remote Write.
    #[cfg(feature = "sinks-prometheus")]
    PrometheusRemoteWrite(#[configurable(derived)] prometheus::remote_write::RemoteWriteConfig),
    */
    /// Apache Pulsar.
    #[cfg(feature = "sinks-pulsar")]
    Pulsar(#[configurable(derived)] pulsar::PulsarSinkConfig),

    /// Redis.
    #[cfg(feature = "sinks-redis")]
    Redis(#[configurable(derived)] redis::RedisSinkConfig),
    /*
    /// Sematext Logs.
    #[cfg(feature = "sinks-sematext")]
    SematextLogs(#[configurable(derived)] sematext::logs::SematextLogsConfig),

    /// Sematext Metrics.
    #[cfg(feature = "sinks-sematext")]
    SematextMetrics(#[configurable(derived)] sematext::metrics::SematextMetricsConfig),

    /// Socket.
    #[cfg(feature = "sinks-socket")]
    Socket(#[configurable(derived)] socket::SocketSinkConfig),

    /// Splunk HEC Logs.
    #[cfg(feature = "sinks-splunk_hec")]
    SplunkHecLogs(#[configurable(derived)] splunk_hec::logs::config::HecLogsSinkConfig),

    /// Splunk HEC Metrics.
    #[cfg(feature = "sinks-splunk_hec")]
    SplunkHecMetrics(#[configurable(derived)] splunk_hec::metrics::config::HecMetricsSinkConfig),

    /// StatsD.
    #[cfg(feature = "sinks-statsd")]
    Statsd(#[configurable(derived)] statsd::StatsdSinkConfig),

    /// Vector.
    #[cfg(feature = "sinks-vector")]
    Vector(#[configurable(derived)] vector::VectorConfig),

    /// Websocket.
    #[cfg(feature = "sinks-websocket")]
    Websocket(#[configurable(derived)] websocket::WebSocketSinkConfig),
    */
}
