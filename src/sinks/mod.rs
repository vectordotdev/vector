#![allow(missing_docs)]
use enum_dispatch::enum_dispatch;
use futures::future::BoxFuture;
use snafu::Snafu;

pub mod util;

#[cfg(feature = "sinks-amqp")]
pub mod amqp;
#[cfg(feature = "sinks-appsignal")]
pub mod appsignal;
#[cfg(feature = "sinks-aws_cloudwatch_logs")]
pub mod aws_cloudwatch_logs;
#[cfg(feature = "sinks-aws_cloudwatch_metrics")]
pub mod aws_cloudwatch_metrics;
#[cfg(any(
    feature = "sinks-aws_kinesis_streams",
    feature = "sinks-aws_kinesis_firehose",
))]
pub mod aws_kinesis;
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
#[cfg(feature = "sinks-databend")]
pub mod databend;
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
#[cfg(feature = "sinks-loki")]
pub mod loki;
#[cfg(feature = "sinks-mezmo")]
pub mod mezmo;
#[cfg(feature = "sinks-nats")]
pub mod nats;
#[cfg(feature = "sinks-new_relic")]
pub mod new_relic;
#[cfg(feature = "sinks-webhdfs")]
pub mod opendal_common;
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
#[cfg(feature = "sinks-webhdfs")]
pub mod webhdfs;
#[cfg(feature = "sinks-websocket")]
pub mod websocket;

use vector_config::{configurable_component, NamedComponent};
pub use vector_core::{config::Input, sink::VectorSink};

use crate::config::{
    unit_test::{UnitTestSinkConfig, UnitTestStreamSinkConfig},
    AcknowledgementsConfig, Resource, SinkConfig, SinkContext,
};

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
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
#[enum_dispatch(SinkConfig)]
pub enum Sinks {
    /// Send events to AMQP 0.9.1 compatible brokers like RabbitMQ.
    #[cfg(feature = "sinks-amqp")]
    Amqp(amqp::AmqpSinkConfig),

    /// Send events to AppSignal.
    #[cfg(feature = "sinks-appsignal")]
    Appsignal(appsignal::AppsignalSinkConfig),

    /// Publish log events to AWS CloudWatch Logs.
    #[cfg(feature = "sinks-aws_cloudwatch_logs")]
    AwsCloudwatchLogs(aws_cloudwatch_logs::CloudwatchLogsSinkConfig),

    /// Publish metric events to AWS CloudWatch Metrics.
    #[cfg(feature = "sinks-aws_cloudwatch_metrics")]
    AwsCloudwatchMetrics(aws_cloudwatch_metrics::CloudWatchMetricsSinkConfig),

    /// Publish logs to AWS Kinesis Data Firehose topics.
    #[cfg(feature = "sinks-aws_kinesis_firehose")]
    #[configurable(metadata(docs::human_name = "AWS Kinesis Data Firehose Logs"))]
    AwsKinesisFirehose(aws_kinesis::firehose::KinesisFirehoseSinkConfig),

    /// Publish logs to AWS Kinesis Streams topics.
    #[cfg(feature = "sinks-aws_kinesis_streams")]
    #[configurable(metadata(docs::human_name = "AWS Kinesis Streams Logs"))]
    AwsKinesisStreams(aws_kinesis::streams::KinesisStreamsSinkConfig),

    /// Store observability events in the AWS S3 object storage system.
    #[cfg(feature = "sinks-aws_s3")]
    AwsS3(aws_s3::S3SinkConfig),

    /// Publish observability events to AWS Simple Queue Service topics.
    #[cfg(feature = "sinks-aws_sqs")]
    AwsSqs(aws_sqs::SqsSinkConfig),

    /// Deliver log events to Axiom.
    #[cfg(feature = "sinks-axiom")]
    Axiom(axiom::AxiomConfig),

    /// Store your observability data in Azure Blob Storage.
    #[cfg(feature = "sinks-azure_blob")]
    #[configurable(metadata(docs::human_name = "Azure Blob Storage"))]
    AzureBlob(azure_blob::AzureBlobSinkConfig),

    /// Publish log events to the Azure Monitor Logs service.
    #[cfg(feature = "sinks-azure_monitor_logs")]
    AzureMonitorLogs(azure_monitor_logs::AzureMonitorLogsConfig),

    /// Send observability events nowhere, which can be useful for debugging purposes.
    #[cfg(feature = "sinks-blackhole")]
    Blackhole(blackhole::BlackholeConfig),

    /// Deliver log data to a ClickHouse database.
    #[cfg(feature = "sinks-clickhouse")]
    Clickhouse(clickhouse::ClickhouseConfig),

    /// Display observability events in the console, which can be useful for debugging purposes.
    #[cfg(feature = "sinks-console")]
    Console(console::ConsoleSinkConfig),

    /// Deliver log data to a Databend database.
    #[cfg(feature = "sinks-databend")]
    Databend(databend::DatabendConfig),

    /// Send events to Datadog Archives.
    #[cfg(feature = "sinks-datadog_archives")]
    DatadogArchives(datadog_archives::DatadogArchivesSinkConfig),

    /// Publish observability events to the Datadog Events API.
    #[cfg(feature = "sinks-datadog_events")]
    DatadogEvents(datadog::events::DatadogEventsConfig),

    /// Publish log events to Datadog.
    #[cfg(feature = "sinks-datadog_logs")]
    DatadogLogs(datadog::logs::DatadogLogsConfig),

    /// Publish metric events to Datadog.
    #[cfg(feature = "sinks-datadog_metrics")]
    DatadogMetrics(datadog::metrics::DatadogMetricsConfig),

    /// Publish traces to Datadog.
    #[cfg(feature = "sinks-datadog_traces")]
    DatadogTraces(datadog::traces::DatadogTracesConfig),

    /// Index observability events in Elasticsearch.
    #[cfg(feature = "sinks-elasticsearch")]
    Elasticsearch(elasticsearch::ElasticsearchConfig),

    /// Output observability events into files.
    #[cfg(feature = "sinks-file")]
    File(file::FileSinkConfig),

    /// Store unstructured log events in Google Chronicle.
    #[cfg(feature = "sinks-gcp")]
    GcpChronicleUnstructured(gcp::chronicle_unstructured::ChronicleUnstructuredConfig),

    /// Deliver logs to GCP's Cloud Operations suite.
    #[cfg(feature = "sinks-gcp")]
    #[configurable(metadata(docs::human_name = "GCP Operations (Stackdriver)"))]
    GcpStackdriverLogs(gcp::stackdriver_logs::StackdriverConfig),

    /// Deliver metrics to GCP's Cloud Monitoring system.
    #[cfg(feature = "sinks-gcp")]
    #[configurable(metadata(docs::human_name = "GCP Cloud Monitoring (Stackdriver)"))]
    GcpStackdriverMetrics(gcp::stackdriver_metrics::StackdriverConfig),

    /// Store observability events in GCP Cloud Storage.
    #[cfg(feature = "sinks-gcp")]
    GcpCloudStorage(gcp::cloud_storage::GcsSinkConfig),

    /// Publish observability events to GCP's Pub/Sub messaging system.
    #[cfg(feature = "sinks-gcp")]
    GcpPubsub(gcp::pubsub::PubsubConfig),

    /// WebHDFS.
    #[cfg(feature = "sinks-webhdfs")]
    Webhdfs(webhdfs::WebHdfsConfig),

    /// Deliver log events to Honeycomb.
    #[cfg(feature = "sinks-honeycomb")]
    Honeycomb(honeycomb::HoneycombConfig),

    /// Deliver observability event data to an HTTP server.
    #[cfg(feature = "sinks-http")]
    Http(http::HttpSinkConfig),

    /// Deliver log event data to Humio.
    #[cfg(feature = "sinks-humio")]
    HumioLogs(humio::logs::HumioLogsConfig),

    /// Deliver metric event data to Humio.
    #[cfg(feature = "sinks-humio")]
    HumioMetrics(humio::metrics::HumioMetricsConfig),

    /// Deliver log event data to InfluxDB.
    #[cfg(any(feature = "sinks-influxdb", feature = "prometheus-integration-tests"))]
    InfluxdbLogs(influxdb::logs::InfluxDbLogsConfig),

    /// Deliver metric event data to InfluxDB.
    #[cfg(any(feature = "sinks-influxdb", feature = "prometheus-integration-tests"))]
    InfluxdbMetrics(influxdb::metrics::InfluxDbConfig),

    /// Publish observability event data to Apache Kafka topics.
    #[cfg(feature = "sinks-kafka")]
    Kafka(kafka::KafkaSinkConfig),

    /// Deliver log event data to Mezmo.
    #[cfg(feature = "sinks-mezmo")]
    Mezmo(mezmo::MezmoConfig),

    /// Deliver log event data to LogDNA.
    #[cfg(feature = "sinks-mezmo")]
    Logdna(mezmo::LogdnaConfig),

    /// Deliver log event data to the Loki aggregation system.
    #[cfg(feature = "sinks-loki")]
    Loki(loki::LokiConfig),

    /// Publish observability data to subjects on the NATS messaging system.
    #[cfg(feature = "sinks-nats")]
    Nats(self::nats::NatsSinkConfig),

    /// Deliver events to New Relic.
    #[cfg(feature = "sinks-new_relic")]
    NewRelic(new_relic::NewRelicConfig),

    /// Deliver log events to Papertrail from SolarWinds.
    #[cfg(feature = "sinks-papertrail")]
    Papertrail(papertrail::PapertrailConfig),

    /// Expose metric events on a Prometheus compatible endpoint.
    #[cfg(feature = "sinks-prometheus")]
    PrometheusExporter(prometheus::exporter::PrometheusExporterConfig),

    /// Deliver metric data to a Prometheus remote write endpoint.
    #[cfg(feature = "sinks-prometheus")]
    PrometheusRemoteWrite(prometheus::remote_write::RemoteWriteConfig),

    /// Publish observability events to Apache Pulsar topics.
    #[cfg(feature = "sinks-pulsar")]
    Pulsar(pulsar::config::PulsarSinkConfig),

    /// Publish observability data to Redis.
    #[cfg(feature = "sinks-redis")]
    Redis(redis::RedisSinkConfig),

    /// Publish log events to Sematext.
    #[cfg(feature = "sinks-sematext")]
    SematextLogs(sematext::logs::SematextLogsConfig),

    /// Publish metric events to Sematext.
    #[cfg(feature = "sinks-sematext")]
    SematextMetrics(sematext::metrics::SematextMetricsConfig),

    /// Deliver logs to a remote socket endpoint.
    #[cfg(feature = "sinks-socket")]
    Socket(socket::SocketSinkConfig),

    /// Deliver log data to Splunk's HTTP Event Collector.
    #[cfg(feature = "sinks-splunk_hec")]
    SplunkHecLogs(splunk_hec::logs::config::HecLogsSinkConfig),

    /// Deliver metric data to Splunk's HTTP Event Collector.
    #[cfg(feature = "sinks-splunk_hec")]
    SplunkHecMetrics(splunk_hec::metrics::config::HecMetricsSinkConfig),

    /// Deliver metric data to a StatsD aggregator.
    #[cfg(feature = "sinks-statsd")]
    Statsd(statsd::StatsdSinkConfig),

    /// Test (adaptive concurrency).
    #[cfg(all(test, feature = "sources-demo_logs"))]
    TestArc(self::util::adaptive_concurrency::tests::TestConfig),

    /// Test (backpressure).
    #[cfg(test)]
    TestBackpressure(crate::test_util::mock::sinks::BackpressureSinkConfig),

    /// Test (basic).
    #[cfg(test)]
    TestBasic(crate::test_util::mock::sinks::BasicSinkConfig),

    /// Test (error).
    #[cfg(test)]
    TestError(crate::test_util::mock::sinks::ErrorSinkConfig),

    /// Test (oneshot).
    #[cfg(test)]
    TestOneshot(crate::test_util::mock::sinks::OneshotSinkConfig),

    /// Test (panic).
    #[cfg(test)]
    TestPanic(crate::test_util::mock::sinks::PanicSinkConfig),

    /// Unit test.
    UnitTest(UnitTestSinkConfig),

    /// Unit test stream.
    UnitTestStream(UnitTestStreamSinkConfig),

    /// Relay observability data to a Vector instance.
    #[cfg(feature = "sinks-vector")]
    Vector(vector::VectorConfig),

    /// Deliver observability event data to a websocket listener.
    #[cfg(feature = "sinks-websocket")]
    Websocket(websocket::WebSocketSinkConfig),
}

impl NamedComponent for Sinks {
    fn get_component_name(&self) -> &'static str {
        match self {
            #[cfg(feature = "sinks-amqp")]
            Self::Amqp(config) => config.get_component_name(),
            #[cfg(feature = "sinks-appsignal")]
            Self::Appsignal(config) => config.get_component_name(),
            #[cfg(feature = "sinks-aws_cloudwatch_logs")]
            Self::AwsCloudwatchLogs(config) => config.get_component_name(),
            #[cfg(feature = "sinks-aws_cloudwatch_metrics")]
            Self::AwsCloudwatchMetrics(config) => config.get_component_name(),
            #[cfg(feature = "sinks-aws_kinesis_firehose")]
            Self::AwsKinesisFirehose(config) => config.get_component_name(),
            #[cfg(feature = "sinks-aws_kinesis_streams")]
            Self::AwsKinesisStreams(config) => config.get_component_name(),
            #[cfg(feature = "sinks-aws_s3")]
            Self::AwsS3(config) => config.get_component_name(),
            #[cfg(feature = "sinks-aws_sqs")]
            Self::AwsSqs(config) => config.get_component_name(),
            #[cfg(feature = "sinks-axiom")]
            Self::Axiom(config) => config.get_component_name(),
            #[cfg(feature = "sinks-azure_blob")]
            Self::AzureBlob(config) => config.get_component_name(),
            #[cfg(feature = "sinks-azure_monitor_logs")]
            Self::AzureMonitorLogs(config) => config.get_component_name(),
            #[cfg(feature = "sinks-blackhole")]
            Self::Blackhole(config) => config.get_component_name(),
            #[cfg(feature = "sinks-clickhouse")]
            Self::Clickhouse(config) => config.get_component_name(),
            #[cfg(feature = "sinks-console")]
            Self::Console(config) => config.get_component_name(),
            #[cfg(feature = "sinks-databend")]
            Self::Databend(config) => config.get_component_name(),
            #[cfg(feature = "sinks-datadog_archives")]
            Self::DatadogArchives(config) => config.get_component_name(),
            #[cfg(feature = "sinks-datadog_events")]
            Self::DatadogEvents(config) => config.get_component_name(),
            #[cfg(feature = "sinks-datadog_logs")]
            Self::DatadogLogs(config) => config.get_component_name(),
            #[cfg(feature = "sinks-datadog_metrics")]
            Self::DatadogMetrics(config) => config.get_component_name(),
            #[cfg(feature = "sinks-datadog_traces")]
            Self::DatadogTraces(config) => config.get_component_name(),
            #[cfg(feature = "sinks-elasticsearch")]
            Self::Elasticsearch(config) => config.get_component_name(),
            #[cfg(feature = "sinks-file")]
            Self::File(config) => config.get_component_name(),
            #[cfg(feature = "sinks-gcp")]
            Self::GcpChronicleUnstructured(config) => config.get_component_name(),
            #[cfg(feature = "sinks-gcp")]
            Self::GcpStackdriverLogs(config) => config.get_component_name(),
            #[cfg(feature = "sinks-gcp")]
            Self::GcpStackdriverMetrics(config) => config.get_component_name(),
            #[cfg(feature = "sinks-gcp")]
            Self::GcpCloudStorage(config) => config.get_component_name(),
            #[cfg(feature = "sinks-gcp")]
            Self::GcpPubsub(config) => config.get_component_name(),
            #[cfg(feature = "sinks-webhdfs")]
            Self::Webhdfs(config) => config.get_component_name(),
            #[cfg(feature = "sinks-honeycomb")]
            Self::Honeycomb(config) => config.get_component_name(),
            #[cfg(feature = "sinks-http")]
            Self::Http(config) => config.get_component_name(),
            #[cfg(feature = "sinks-humio")]
            Self::HumioLogs(config) => config.get_component_name(),
            #[cfg(feature = "sinks-humio")]
            Self::HumioMetrics(config) => config.get_component_name(),
            #[cfg(any(feature = "sinks-influxdb", feature = "prometheus-integration-tests"))]
            Self::InfluxdbLogs(config) => config.get_component_name(),
            #[cfg(any(feature = "sinks-influxdb", feature = "prometheus-integration-tests"))]
            Self::InfluxdbMetrics(config) => config.get_component_name(),
            #[cfg(feature = "sinks-kafka")]
            Self::Kafka(config) => config.get_component_name(),
            #[cfg(feature = "sinks-mezmo")]
            Self::Mezmo(config) => config.get_component_name(),
            #[cfg(feature = "sinks-mezmo")]
            Self::Logdna(config) => config.get_component_name(),
            #[cfg(feature = "sinks-loki")]
            Self::Loki(config) => config.get_component_name(),
            #[cfg(feature = "sinks-nats")]
            Self::Nats(config) => config.get_component_name(),
            #[cfg(feature = "sinks-new_relic")]
            Self::NewRelic(config) => config.get_component_name(),
            #[cfg(feature = "sinks-papertrail")]
            Self::Papertrail(config) => config.get_component_name(),
            #[cfg(feature = "sinks-prometheus")]
            Self::PrometheusExporter(config) => config.get_component_name(),
            #[cfg(feature = "sinks-prometheus")]
            Self::PrometheusRemoteWrite(config) => config.get_component_name(),
            #[cfg(feature = "sinks-pulsar")]
            Self::Pulsar(config) => config.get_component_name(),
            #[cfg(feature = "sinks-redis")]
            Self::Redis(config) => config.get_component_name(),
            #[cfg(feature = "sinks-sematext")]
            Self::SematextLogs(config) => config.get_component_name(),
            #[cfg(feature = "sinks-sematext")]
            Self::SematextMetrics(config) => config.get_component_name(),
            #[cfg(feature = "sinks-socket")]
            Self::Socket(config) => config.get_component_name(),
            #[cfg(feature = "sinks-splunk_hec")]
            Self::SplunkHecLogs(config) => config.get_component_name(),
            #[cfg(feature = "sinks-splunk_hec")]
            Self::SplunkHecMetrics(config) => config.get_component_name(),
            #[cfg(feature = "sinks-statsd")]
            Self::Statsd(config) => config.get_component_name(),
            #[cfg(all(test, feature = "sources-demo_logs"))]
            Self::TestArc(config) => config.get_component_name(),
            #[cfg(test)]
            Self::TestBackpressure(config) => config.get_component_name(),
            #[cfg(test)]
            Self::TestBasic(config) => config.get_component_name(),
            #[cfg(test)]
            Self::TestError(config) => config.get_component_name(),
            #[cfg(test)]
            Self::TestOneshot(config) => config.get_component_name(),
            #[cfg(test)]
            Self::TestPanic(config) => config.get_component_name(),
            Self::UnitTest(config) => config.get_component_name(),
            Self::UnitTestStream(config) => config.get_component_name(),
            #[cfg(feature = "sinks-vector")]
            Self::Vector(config) => config.get_component_name(),
            #[cfg(feature = "sinks-websocket")]
            Self::Websocket(config) => config.get_component_name(),
        }
    }
}
