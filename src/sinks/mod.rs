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
    #[configurable(metadata(docs::label = "AMQP"))]
    Amqp(amqp::AmqpSinkConfig),

    /// Send events to AppSignal.
    #[cfg(feature = "sinks-appsignal")]
    #[configurable(metadata(docs::label = "AppSignal"))]
    Appsignal(appsignal::AppsignalSinkConfig),

    /// Publish log events to AWS CloudWatch Logs.
    #[cfg(feature = "sinks-aws_cloudwatch_logs")]
    #[configurable(metadata(docs::label = "AWS CloudWatch Logs"))]
    AwsCloudwatchLogs(aws_cloudwatch_logs::CloudwatchLogsSinkConfig),

    /// Publish metric events to AWS CloudWatch Metrics.
    #[cfg(feature = "sinks-aws_cloudwatch_metrics")]
    #[configurable(metadata(docs::label = "AWS CloudWatch Metrics"))]
    AwsCloudwatchMetrics(aws_cloudwatch_metrics::CloudWatchMetricsSinkConfig),

    /// Publish logs to AWS Kinesis Data Firehose topics.
    #[cfg(feature = "sinks-aws_kinesis_firehose")]
    #[configurable(metadata(docs::label = "AWS Kinesis Data Firehose Logs"))]
    AwsKinesisFirehose(aws_kinesis::firehose::KinesisFirehoseSinkConfig),

    /// Publish logs to AWS Kinesis Streams topics.
    #[cfg(feature = "sinks-aws_kinesis_streams")]
    #[configurable(metadata(docs::label = "AWS Kinesis Streams Logs"))]
    AwsKinesisStreams(aws_kinesis::streams::KinesisStreamsSinkConfig),

    /// Store observability events in the AWS S3 object storage system.
    #[cfg(feature = "sinks-aws_s3")]
    #[configurable(metadata(docs::label = "AWS S3"))]
    AwsS3(aws_s3::S3SinkConfig),

    /// Publish observability events to AWS Simple Queue Service topics.
    #[cfg(feature = "sinks-aws_sqs")]
    #[configurable(metadata(docs::label = "AWS SQS"))]
    AwsSqs(aws_sqs::SqsSinkConfig),

    /// Deliver log events to Axiom.
    #[cfg(feature = "sinks-axiom")]
    #[configurable(metadata(docs::label = "Axiom"))]
    Axiom(axiom::AxiomConfig),

    /// Store your observability data in Azure Blob Storage.
    #[cfg(feature = "sinks-azure_blob")]
    #[configurable(metadata(docs::label = "Azure Blob Storage"))]
    AzureBlob(azure_blob::AzureBlobSinkConfig),

    /// Publish log events to the Azure Monitor Logs service.
    #[cfg(feature = "sinks-azure_monitor_logs")]
    #[configurable(metadata(docs::label = "Azure Monitor Logs"))]
    AzureMonitorLogs(azure_monitor_logs::AzureMonitorLogsConfig),

    /// Send observability events nowhere, which can be useful for debugging purposes.
    #[cfg(feature = "sinks-blackhole")]
    #[configurable(metadata(docs::label = "Blackhole"))]
    Blackhole(blackhole::BlackholeConfig),

    /// Deliver log data to a ClickHouse database.
    #[cfg(feature = "sinks-clickhouse")]
    #[configurable(metadata(docs::label = "ClickHouse"))]
    Clickhouse(clickhouse::ClickhouseConfig),

    /// Display observability events in the console, which can be useful for debugging purposes.
    #[cfg(feature = "sinks-console")]
    #[configurable(metadata(docs::label = "Console"))]
    Console(console::ConsoleSinkConfig),

    /// Deliver log data to a Databend database.
    #[cfg(feature = "sinks-databend")]
    Databend(databend::DatabendConfig),

    /// Send events to Datadog Archives.
    #[cfg(feature = "sinks-datadog_archives")]
    #[configurable(metadata(docs::label = "Datadog Archives"))]
    DatadogArchives(datadog_archives::DatadogArchivesSinkConfig),

    /// Publish observability events to the Datadog Events API.
    #[cfg(feature = "sinks-datadog_events")]
    #[configurable(metadata(docs::label = "Datadog Events"))]
    DatadogEvents(datadog::events::DatadogEventsConfig),

    /// Publish log events to Datadog.
    #[cfg(feature = "sinks-datadog_logs")]
    #[configurable(metadata(docs::label = "Datadog Logs"))]
    DatadogLogs(datadog::logs::DatadogLogsConfig),

    /// Publish metric events to Datadog.
    #[cfg(feature = "sinks-datadog_metrics")]
    #[configurable(metadata(docs::label = "Datadog Metrics"))]
    DatadogMetrics(datadog::metrics::DatadogMetricsConfig),

    /// Publish traces to Datadog.
    #[cfg(feature = "sinks-datadog_traces")]
    #[configurable(metadata(docs::label = "Datadog Traces"))]
    DatadogTraces(datadog::traces::DatadogTracesConfig),

    /// Index observability events in Elasticsearch.
    #[cfg(feature = "sinks-elasticsearch")]
    #[configurable(metadata(docs::label = "Elasticsearch"))]
    Elasticsearch(elasticsearch::ElasticsearchConfig),

    /// Output observability events into files.
    #[cfg(feature = "sinks-file")]
    #[configurable(metadata(docs::label = "File"))]
    File(file::FileSinkConfig),

    /// Store unstructured log events in Google Chronicle.
    #[cfg(feature = "sinks-gcp")]
    #[configurable(metadata(docs::label = "GCP Chronicle Unstructured"))]
    GcpChronicleUnstructured(gcp::chronicle_unstructured::ChronicleUnstructuredConfig),

    /// Deliver logs to GCP's Cloud Operations suite.
    #[cfg(feature = "sinks-gcp")]
    #[configurable(metadata(docs::label = "GCP Operations (Stackdriver)"))]
    GcpStackdriverLogs(gcp::stackdriver_logs::StackdriverConfig),

    /// Deliver metrics to GCP's Cloud Monitoring system.
    #[cfg(feature = "sinks-gcp")]
    #[configurable(metadata(docs::label = "GCP Cloud Monitoring (Stackdriver)"))]
    GcpStackdriverMetrics(gcp::stackdriver_metrics::StackdriverConfig),

    /// Store observability events in GCP Cloud Storage.
    #[cfg(feature = "sinks-gcp")]
    #[configurable(metadata(docs::label = "GCP Cloud Storage"))]
    GcpCloudStorage(gcp::cloud_storage::GcsSinkConfig),

    /// Publish observability events to GCP's Pub/Sub messaging system.
    #[cfg(feature = "sinks-gcp")]
    #[configurable(metadata(docs::label = "GCP Pub/Sub"))]
    GcpPubsub(gcp::pubsub::PubsubConfig),

    /// WebHDFS.
    #[cfg(feature = "sinks-webhdfs")]
    #[configurable(metadata(docs::label = "WebHDFS"))]
    Webhdfs(webhdfs::WebHdfsConfig),

    /// Deliver log events to Honeycomb.
    #[cfg(feature = "sinks-honeycomb")]
    #[configurable(metadata(docs::label = "Honeycomb"))]
    Honeycomb(honeycomb::HoneycombConfig),

    /// Deliver observability event data to an HTTP server.
    #[cfg(feature = "sinks-http")]
    #[configurable(metadata(docs::label = "HTTP"))]
    Http(http::HttpSinkConfig),

    /// Deliver log event data to Humio.
    #[cfg(feature = "sinks-humio")]
    #[configurable(metadata(docs::label = "Humio Logs"))]
    HumioLogs(humio::logs::HumioLogsConfig),

    /// Deliver metric event data to Humio.
    #[cfg(feature = "sinks-humio")]
    #[configurable(metadata(docs::label = "Humio Metrics"))]
    HumioMetrics(humio::metrics::HumioMetricsConfig),

    /// Deliver log event data to InfluxDB.
    #[cfg(any(feature = "sinks-influxdb", feature = "prometheus-integration-tests"))]
    #[configurable(metadata(docs::label = "InfluxDB Logs"))]
    InfluxdbLogs(influxdb::logs::InfluxDbLogsConfig),

    /// Deliver metric event data to InfluxDB.
    #[cfg(any(feature = "sinks-influxdb", feature = "prometheus-integration-tests"))]
    #[configurable(metadata(docs::label = "InfluxDB Metrics"))]
    InfluxdbMetrics(influxdb::metrics::InfluxDbConfig),

    /// Publish observability event data to Apache Kafka topics.
    #[cfg(feature = "sinks-kafka")]
    #[configurable(metadata(docs::label = "Kafka"))]
    Kafka(kafka::KafkaSinkConfig),

    /// Deliver log event data to Mezmo.
    #[cfg(feature = "sinks-mezmo")]
    #[configurable(metadata(docs::label = "Mezmo"))]
    Mezmo(mezmo::MezmoConfig),

    /// Deliver log event data to LogDNA.
    #[cfg(feature = "sinks-mezmo")]
    #[configurable(metadata(docs::label = "LogDNA"))]
    Logdna(mezmo::LogdnaConfig),

    /// Deliver log event data to the Loki aggregation system.
    #[cfg(feature = "sinks-loki")]
    #[configurable(metadata(docs::label = "Loki"))]
    Loki(loki::LokiConfig),

    /// Publish observability data to subjects on the NATS messaging system.
    #[cfg(feature = "sinks-nats")]
    #[configurable(metadata(docs::label = "NATS"))]
    Nats(self::nats::NatsSinkConfig),

    /// Deliver events to New Relic.
    #[cfg(feature = "sinks-new_relic")]
    #[configurable(metadata(docs::label = "New Relic"))]
    NewRelic(new_relic::NewRelicConfig),

    /// Deliver log events to Papertrail from SolarWinds.
    #[cfg(feature = "sinks-papertrail")]
    #[configurable(metadata(docs::label = "Papertrail"))]
    Papertrail(papertrail::PapertrailConfig),

    /// Expose metric events on a Prometheus compatible endpoint.
    #[cfg(feature = "sinks-prometheus")]
    #[configurable(metadata(docs::label = "Prometheus Exporter"))]
    PrometheusExporter(prometheus::exporter::PrometheusExporterConfig),

    /// Deliver metric data to a Prometheus remote write endpoint.
    #[cfg(feature = "sinks-prometheus")]
    #[configurable(metadata(docs::label = "Prometheus Remote Write"))]
    PrometheusRemoteWrite(prometheus::remote_write::RemoteWriteConfig),

    /// Publish observability events to Apache Pulsar topics.
    #[cfg(feature = "sinks-pulsar")]
    #[configurable(metadata(docs::label = "Pulsar"))]
    Pulsar(pulsar::PulsarSinkConfig),

    /// Publish observability data to Redis.
    #[cfg(feature = "sinks-redis")]
    #[configurable(metadata(docs::label = "Redis"))]
    Redis(redis::RedisSinkConfig),

    /// Publish log events to Sematext.
    #[cfg(feature = "sinks-sematext")]
    #[configurable(metadata(docs::label = "Sematext Logs"))]
    SematextLogs(sematext::logs::SematextLogsConfig),

    /// Publish metric events to Sematext.
    #[cfg(feature = "sinks-sematext")]
    #[configurable(metadata(docs::label = "Sematext Metrics"))]
    SematextMetrics(sematext::metrics::SematextMetricsConfig),

    /// Deliver logs to a remote socket endpoint.
    #[cfg(feature = "sinks-socket")]
    #[configurable(metadata(docs::label = "Socket"))]
    Socket(socket::SocketSinkConfig),

    /// Deliver log data to Splunk's HTTP Event Collector.
    #[cfg(feature = "sinks-splunk_hec")]
    #[configurable(metadata(docs::label = "Splunk HEC Logs"))]
    SplunkHecLogs(splunk_hec::logs::config::HecLogsSinkConfig),

    /// Deliver metric data to Splunk's HTTP Event Collector.
    #[cfg(feature = "sinks-splunk_hec")]
    #[configurable(metadata(docs::label = "Splunk HEC Metrics"))]
    SplunkHecMetrics(splunk_hec::metrics::config::HecMetricsSinkConfig),

    /// Deliver metric data to a StatsD aggregator.
    #[cfg(feature = "sinks-statsd")]
    #[configurable(metadata(docs::label = "Statsd"))]
    Statsd(statsd::StatsdSinkConfig),

    /// Test (adaptive concurrency).
    #[cfg(all(test, feature = "sources-demo_logs"))]
    #[configurable(metadata(docs::label = ""))]
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
    #[configurable(metadata(docs::label = "Vector"))]
    Vector(vector::VectorConfig),

    /// Deliver observability event data to a websocket listener.
    #[cfg(feature = "sinks-websocket")]
    #[configurable(metadata(docs::label = "Websocket"))]
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
