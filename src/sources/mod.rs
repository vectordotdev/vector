use snafu::Snafu;

#[cfg(feature = "sources-apache_metrics")]
pub mod apache_metrics;
#[cfg(feature = "sources-aws_ecs_metrics")]
pub mod aws_ecs_metrics;
#[cfg(feature = "sources-aws_kinesis_firehose")]
pub mod aws_kinesis_firehose;
#[cfg(feature = "sources-aws_s3")]
pub mod aws_s3;
#[cfg(feature = "sources-aws_sqs")]
pub mod aws_sqs;
#[cfg(any(feature = "sources-datadog_agent"))]
pub mod datadog_agent;
#[cfg(feature = "sources-demo_logs")]
pub mod demo_logs;
#[cfg(all(unix, feature = "sources-dnstap"))]
pub mod dnstap;
#[cfg(feature = "sources-docker_logs")]
pub mod docker_logs;
#[cfg(feature = "sources-eventstoredb_metrics")]
pub mod eventstoredb_metrics;
#[cfg(feature = "sources-exec")]
pub mod exec;
#[cfg(feature = "sources-file")]
pub mod file;
#[cfg(any(
    feature = "sources-stdin",
    all(unix, feature = "sources-file-descriptor")
))]
pub mod file_descriptors;
#[cfg(feature = "sources-fluent")]
pub mod fluent;
#[cfg(feature = "sources-gcp_pubsub")]
pub mod gcp_pubsub;
#[cfg(feature = "sources-heroku_logs")]
pub mod heroku_logs;
#[cfg(feature = "sources-host_metrics")]
pub mod host_metrics;
#[cfg(feature = "sources-http")]
pub mod http;
#[cfg(feature = "sources-http_scrape")]
pub mod http_scrape;
#[cfg(feature = "sources-internal_logs")]
pub mod internal_logs;
#[cfg(feature = "sources-internal_metrics")]
pub mod internal_metrics;
#[cfg(all(unix, feature = "sources-journald"))]
pub mod journald;
#[cfg(feature = "sources-kafka")]
pub mod kafka;
#[cfg(feature = "sources-kubernetes_logs")]
pub mod kubernetes_logs;
#[cfg(all(feature = "sources-logstash"))]
pub mod logstash;
#[cfg(feature = "sources-mongodb_metrics")]
pub mod mongodb_metrics;
#[cfg(all(feature = "sources-nats"))]
pub mod nats;
#[cfg(feature = "sources-nginx_metrics")]
pub mod nginx_metrics;
#[cfg(feature = "sources-opentelemetry")]
pub mod opentelemetry;
#[cfg(feature = "sources-postgresql_metrics")]
pub mod postgresql_metrics;
#[cfg(feature = "sources-prometheus")]
pub mod prometheus;
#[cfg(feature = "sources-redis")]
pub mod redis;
#[cfg(feature = "sources-socket")]
pub mod socket;
#[cfg(feature = "sources-splunk_hec")]
pub mod splunk_hec;
#[cfg(feature = "sources-statsd")]
pub mod statsd;
#[cfg(feature = "sources-syslog")]
pub mod syslog;
#[cfg(feature = "sources-vector")]
pub mod vector;

pub(crate) mod util;

use vector_config::{configurable_component, NamedComponent};
use vector_core::config::{LogNamespace, Output};
pub use vector_core::source::Source;

use crate::config::{unit_test::UnitTestSourceConfig, Resource, SourceConfig, SourceContext};

/// Common build errors
#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("URI parse error: {}", source))]
    UriParseError { source: ::http::uri::InvalidUri },
}

/// Configurable sources in Vector.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Sources {
    /// Apache HTTP Server (HTTPD) Metrics.
    #[cfg(feature = "sources-apache_metrics")]
    ApacheMetrics(#[configurable(derived)] apache_metrics::ApacheMetricsConfig),

    /// AWS ECS Metrics.
    #[cfg(feature = "sources-aws_ecs_metrics")]
    AwsEcsMetrics(#[configurable(derived)] aws_ecs_metrics::AwsEcsMetricsSourceConfig),

    /// AWS Kinesis Firehose.
    #[cfg(feature = "sources-aws_kinesis_firehose")]
    AwsKinesisFirehose(#[configurable(derived)] aws_kinesis_firehose::AwsKinesisFirehoseConfig),

    /// AWS S3.
    #[cfg(feature = "sources-aws_s3")]
    AwsS3(#[configurable(derived)] aws_s3::AwsS3Config),

    /// AWS SQS.
    #[cfg(feature = "sources-aws_sqs")]
    AwsSqs(#[configurable(derived)] aws_sqs::AwsSqsConfig),

    /// Datadog Agent.
    #[cfg(feature = "sources-datadog_agent")]
    DatadogAgent(#[configurable(derived)] datadog_agent::DatadogAgentConfig),

    /// Demo logs.
    #[cfg(feature = "sources-demo_logs")]
    DemoLogs(#[configurable(derived)] demo_logs::DemoLogsConfig),

    /// DNSTAP.
    #[cfg(all(unix, feature = "sources-dnstap"))]
    Dnstap(#[configurable(derived)] dnstap::DnstapConfig),

    /// Docker Logs.
    #[cfg(feature = "sources-docker_logs")]
    DockerLogs(#[configurable(derived)] docker_logs::DockerLogsConfig),

    /// EventStoreDB Metrics.
    #[cfg(feature = "sources-eventstoredb_metrics")]
    EventstoredbMetrics(#[configurable(derived)] eventstoredb_metrics::EventStoreDbConfig),

    /// Exec.
    #[cfg(feature = "sources-exec")]
    Exec(#[configurable(derived)] exec::ExecConfig),

    /// File.
    #[cfg(feature = "sources-file")]
    File(#[configurable(derived)] file::FileConfig),

    /// File descriptor.
    #[cfg(all(unix, feature = "sources-file-descriptor"))]
    FileDescriptor(
        #[configurable(derived)] file_descriptors::file_descriptor::FileDescriptorSourceConfig,
    ),

    /// Fluent.
    #[cfg(feature = "sources-fluent")]
    Fluent(#[configurable(derived)] fluent::FluentConfig),

    /// GCP Pub/Sub.
    #[cfg(feature = "sources-gcp_pubsub")]
    GcpPubsub(#[configurable(derived)] gcp_pubsub::PubsubConfig),

    /// Heroku Logs.
    #[cfg(feature = "sources-heroku_logs")]
    HerokuLogs(#[configurable(derived)] heroku_logs::LogplexConfig),

    /// Host Metrics.
    #[cfg(feature = "sources-host_metrics")]
    HostMetrics(#[configurable(derived)] host_metrics::HostMetricsConfig),

    /// HTTP.
    #[cfg(feature = "sources-http")]
    Http(#[configurable(derived)] http::SimpleHttpConfig),

    /// HTTP Scrape.
    #[cfg(feature = "sources-http_scrape")]
    HttpScrape(#[configurable(derived)] http_scrape::HttpScrapeConfig),

    /// Internal Logs.
    #[cfg(feature = "sources-internal_logs")]
    InternalLogs(#[configurable(derived)] internal_logs::InternalLogsConfig),

    /// Internal Metrics.
    #[cfg(feature = "sources-internal_metrics")]
    InternalMetrics(#[configurable(derived)] internal_metrics::InternalMetricsConfig),

    /// Journald.
    #[cfg(all(unix, feature = "sources-journald"))]
    Journald(#[configurable(derived)] journald::JournaldConfig),

    /// Kafka.
    #[cfg(feature = "sources-kafka")]
    Kafka(#[configurable(derived)] kafka::KafkaSourceConfig),

    /// Kubernetes Logs.
    #[cfg(feature = "sources-kubernetes_logs")]
    KubernetesLogs(#[configurable(derived)] kubernetes_logs::Config),

    /// Logstash.
    #[cfg(all(feature = "sources-logstash"))]
    Logstash(#[configurable(derived)] logstash::LogstashConfig),

    /// MongoDB Metrics.
    #[cfg(feature = "sources-mongodb_metrics")]
    MongodbMetrics(#[configurable(derived)] mongodb_metrics::MongoDbMetricsConfig),

    /// NATS.
    #[cfg(all(feature = "sources-nats"))]
    Nats(#[configurable(derived)] nats::NatsSourceConfig),

    /// NGINX Metrics.
    #[cfg(feature = "sources-nginx_metrics")]
    NginxMetrics(#[configurable(derived)] nginx_metrics::NginxMetricsConfig),

    /// OpenTelemetry.
    #[cfg(feature = "sources-opentelemetry")]
    Opentelemetry(#[configurable(derived)] opentelemetry::OpentelemetryConfig),

    /// PostgreSQL Metrics.
    #[cfg(feature = "sources-postgresql_metrics")]
    PostgresqlMetrics(#[configurable(derived)] postgresql_metrics::PostgresqlMetricsConfig),

    /// Prometheus Scrape.
    #[cfg(feature = "sources-prometheus")]
    PrometheusScrape(#[configurable(derived)] prometheus::PrometheusScrapeConfig),

    /// Prometheus Remote Write.
    #[cfg(feature = "sources-prometheus")]
    PrometheusRemoteWrite(#[configurable(derived)] prometheus::PrometheusRemoteWriteConfig),

    /// Redis.
    #[cfg(feature = "sources-redis")]
    Redis(#[configurable(derived)] redis::RedisSourceConfig),

    /// Test (backpressure).
    #[cfg(test)]
    TestBackpressure(
        #[configurable(derived)] crate::test_util::mock::sources::BackpressureSourceConfig,
    ),

    /// Test (basic).
    #[cfg(test)]
    TestBasic(#[configurable(derived)] crate::test_util::mock::sources::BasicSourceConfig),

    /// Test (error).
    #[cfg(test)]
    TestError(#[configurable(derived)] crate::test_util::mock::sources::ErrorSourceConfig),

    /// Test (panic).
    #[cfg(test)]
    TestPanic(#[configurable(derived)] crate::test_util::mock::sources::PanicSourceConfig),

    /// Test (tripwire).
    #[cfg(test)]
    TestTripwire(#[configurable(derived)] crate::test_util::mock::sources::TripwireSourceConfig),

    /// Socket.
    #[cfg(feature = "sources-socket")]
    Socket(#[configurable(derived)] socket::SocketConfig),

    /// Splunk HEC.
    #[cfg(feature = "sources-splunk_hec")]
    SplunkHec(#[configurable(derived)] splunk_hec::SplunkConfig),

    /// Statsd.
    #[cfg(feature = "sources-statsd")]
    Statsd(#[configurable(derived)] statsd::StatsdConfig),

    /// Stdin.
    #[cfg(feature = "sources-stdin")]
    Stdin(#[configurable(derived)] file_descriptors::stdin::StdinConfig),

    /// Syslog.
    #[cfg(feature = "sources-syslog")]
    Syslog(#[configurable(derived)] syslog::SyslogConfig),

    /// Unit test.
    UnitTest(#[configurable(derived)] UnitTestSourceConfig),

    /// Vector.
    #[cfg(feature = "sources-vector")]
    Vector(#[configurable(derived)] vector::VectorConfig),
}

#[async_trait::async_trait]
impl SourceConfig for Sources {
    async fn build(&self, cx: SourceContext) -> crate::Result<self::Source> {
        match self {
            #[cfg(feature = "sources-apache_metrics")]
            Self::ApacheMetrics(config) => config.build(cx).await,
            #[cfg(feature = "sources-aws_ecs_metrics")]
            Self::AwsEcsMetrics(config) => config.build(cx).await,
            #[cfg(feature = "sources-aws_kinesis_firehose")]
            Self::AwsKinesisFirehose(config) => config.build(cx).await,
            #[cfg(feature = "sources-aws_s3")]
            Self::AwsS3(config) => config.build(cx).await,
            #[cfg(feature = "sources-aws_sqs")]
            Self::AwsSqs(config) => config.build(cx).await,
            #[cfg(feature = "sources-datadog_agent")]
            Self::DatadogAgent(config) => config.build(cx).await,
            #[cfg(feature = "sources-demo_logs")]
            Self::DemoLogs(config) => config.build(cx).await,
            #[cfg(all(unix, feature = "sources-dnstap"))]
            Self::Dnstap(config) => config.build(cx).await,
            #[cfg(feature = "sources-docker_logs")]
            Self::DockerLogs(config) => config.build(cx).await,
            #[cfg(feature = "sources-eventstoredb_metrics")]
            Self::EventstoredbMetrics(config) => config.build(cx).await,
            #[cfg(feature = "sources-exec")]
            Self::Exec(config) => config.build(cx).await,
            #[cfg(feature = "sources-file")]
            Self::File(config) => config.build(cx).await,
            #[cfg(all(unix, feature = "sources-file-descriptor"))]
            Self::FileDescriptor(config) => config.build(cx).await,
            #[cfg(feature = "sources-fluent")]
            Self::Fluent(config) => config.build(cx).await,
            #[cfg(feature = "sources-gcp_pubsub")]
            Self::GcpPubsub(config) => config.build(cx).await,
            #[cfg(feature = "sources-heroku_logs")]
            Self::HerokuLogs(config) => config.build(cx).await,
            #[cfg(feature = "sources-host_metrics")]
            Self::HostMetrics(config) => config.build(cx).await,
            #[cfg(feature = "sources-http")]
            Self::Http(config) => config.build(cx).await,
            #[cfg(feature = "sources-http_scrape")]
            Self::HttpScrape(config) => config.build(cx).await,
            #[cfg(feature = "sources-internal_logs")]
            Self::InternalLogs(config) => config.build(cx).await,
            #[cfg(feature = "sources-internal_metrics")]
            Self::InternalMetrics(config) => config.build(cx).await,
            #[cfg(all(unix, feature = "sources-journald"))]
            Self::Journald(config) => config.build(cx).await,
            #[cfg(feature = "sources-kafka")]
            Self::Kafka(config) => config.build(cx).await,
            #[cfg(feature = "sources-kubernetes_logs")]
            Self::KubernetesLogs(config) => config.build(cx).await,
            #[cfg(all(feature = "sources-logstash"))]
            Self::Logstash(config) => config.build(cx).await,
            #[cfg(feature = "sources-mongodb_metrics")]
            Self::MongodbMetrics(config) => config.build(cx).await,
            #[cfg(all(feature = "sources-nats"))]
            Self::Nats(config) => config.build(cx).await,
            #[cfg(feature = "sources-nginx_metrics")]
            Self::NginxMetrics(config) => config.build(cx).await,
            #[cfg(feature = "sources-opentelemetry")]
            Self::Opentelemetry(config) => config.build(cx).await,
            #[cfg(feature = "sources-postgresql_metrics")]
            Self::PostgresqlMetrics(config) => config.build(cx).await,
            #[cfg(feature = "sources-prometheus")]
            Self::PrometheusScrape(config) => config.build(cx).await,
            #[cfg(feature = "sources-prometheus")]
            Self::PrometheusRemoteWrite(config) => config.build(cx).await,
            #[cfg(feature = "sources-redis")]
            Self::Redis(config) => config.build(cx).await,
            #[cfg(test)]
            Self::TestBackpressure(config) => config.build(cx).await,
            #[cfg(test)]
            Self::TestBasic(config) => config.build(cx).await,
            #[cfg(test)]
            Self::TestError(config) => config.build(cx).await,
            #[cfg(test)]
            Self::TestPanic(config) => config.build(cx).await,
            #[cfg(test)]
            Self::TestTripwire(config) => config.build(cx).await,
            #[cfg(feature = "sources-socket")]
            Self::Socket(config) => config.build(cx).await,
            #[cfg(feature = "sources-splunk_hec")]
            Self::SplunkHec(config) => config.build(cx).await,
            #[cfg(feature = "sources-statsd")]
            Self::Statsd(config) => config.build(cx).await,
            #[cfg(feature = "sources-stdin")]
            Self::Stdin(config) => config.build(cx).await,
            #[cfg(feature = "sources-syslog")]
            Self::Syslog(config) => config.build(cx).await,
            Self::UnitTest(config) => config.build(cx).await,
            #[cfg(feature = "sources-vector")]
            Self::Vector(config) => config.build(cx).await,
        }
    }

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<Output> {
        match self {
            #[cfg(feature = "sources-apache_metrics")]
            Self::ApacheMetrics(config) => config.outputs(global_log_namespace),
            #[cfg(feature = "sources-aws_ecs_metrics")]
            Self::AwsEcsMetrics(config) => config.outputs(global_log_namespace),
            #[cfg(feature = "sources-aws_kinesis_firehose")]
            Self::AwsKinesisFirehose(config) => config.outputs(global_log_namespace),
            #[cfg(feature = "sources-aws_s3")]
            Self::AwsS3(config) => config.outputs(global_log_namespace),
            #[cfg(feature = "sources-aws_sqs")]
            Self::AwsSqs(config) => config.outputs(global_log_namespace),
            #[cfg(feature = "sources-datadog_agent")]
            Self::DatadogAgent(config) => config.outputs(global_log_namespace),
            #[cfg(feature = "sources-demo_logs")]
            Self::DemoLogs(config) => config.outputs(global_log_namespace),
            #[cfg(all(unix, feature = "sources-dnstap"))]
            Self::Dnstap(config) => config.outputs(global_log_namespace),
            #[cfg(feature = "sources-docker_logs")]
            Self::DockerLogs(config) => config.outputs(global_log_namespace),
            #[cfg(feature = "sources-eventstoredb_metrics")]
            Self::EventstoredbMetrics(config) => config.outputs(global_log_namespace),
            #[cfg(feature = "sources-exec")]
            Self::Exec(config) => config.outputs(global_log_namespace),
            #[cfg(feature = "sources-file")]
            Self::File(config) => config.outputs(global_log_namespace),
            #[cfg(all(unix, feature = "sources-file-descriptor"))]
            Self::FileDescriptor(config) => config.outputs(global_log_namespace),
            #[cfg(feature = "sources-fluent")]
            Self::Fluent(config) => config.outputs(global_log_namespace),
            #[cfg(feature = "sources-gcp_pubsub")]
            Self::GcpPubsub(config) => config.outputs(global_log_namespace),
            #[cfg(feature = "sources-heroku_logs")]
            Self::HerokuLogs(config) => config.outputs(global_log_namespace),
            #[cfg(feature = "sources-host_metrics")]
            Self::HostMetrics(config) => config.outputs(global_log_namespace),
            #[cfg(feature = "sources-http")]
            Self::Http(config) => config.outputs(global_log_namespace),
            #[cfg(feature = "sources-http_scrape")]
            Self::HttpScrape(config) => config.outputs(global_log_namespace),
            #[cfg(feature = "sources-internal_logs")]
            Self::InternalLogs(config) => config.outputs(global_log_namespace),
            #[cfg(feature = "sources-internal_metrics")]
            Self::InternalMetrics(config) => config.outputs(global_log_namespace),
            #[cfg(all(unix, feature = "sources-journald"))]
            Self::Journald(config) => config.outputs(global_log_namespace),
            #[cfg(feature = "sources-kafka")]
            Self::Kafka(config) => config.outputs(global_log_namespace),
            #[cfg(feature = "sources-kubernetes_logs")]
            Self::KubernetesLogs(config) => config.outputs(global_log_namespace),
            #[cfg(all(feature = "sources-logstash"))]
            Self::Logstash(config) => config.outputs(global_log_namespace),
            #[cfg(feature = "sources-mongodb_metrics")]
            Self::MongodbMetrics(config) => config.outputs(global_log_namespace),
            #[cfg(all(feature = "sources-nats"))]
            Self::Nats(config) => config.outputs(global_log_namespace),
            #[cfg(feature = "sources-nginx_metrics")]
            Self::NginxMetrics(config) => config.outputs(global_log_namespace),
            #[cfg(feature = "sources-opentelemetry")]
            Self::Opentelemetry(config) => config.outputs(global_log_namespace),
            #[cfg(feature = "sources-postgresql_metrics")]
            Self::PostgresqlMetrics(config) => config.outputs(global_log_namespace),
            #[cfg(feature = "sources-prometheus")]
            Self::PrometheusScrape(config) => config.outputs(global_log_namespace),
            #[cfg(feature = "sources-prometheus")]
            Self::PrometheusRemoteWrite(config) => config.outputs(global_log_namespace),
            #[cfg(feature = "sources-redis")]
            Self::Redis(config) => config.outputs(global_log_namespace),
            #[cfg(test)]
            Self::TestBackpressure(config) => config.outputs(global_log_namespace),
            #[cfg(test)]
            Self::TestBasic(config) => config.outputs(global_log_namespace),
            #[cfg(test)]
            Self::TestError(config) => config.outputs(global_log_namespace),
            #[cfg(test)]
            Self::TestPanic(config) => config.outputs(global_log_namespace),
            #[cfg(test)]
            Self::TestTripwire(config) => config.outputs(global_log_namespace),
            #[cfg(feature = "sources-socket")]
            Self::Socket(config) => config.outputs(global_log_namespace),
            #[cfg(feature = "sources-splunk_hec")]
            Self::SplunkHec(config) => config.outputs(global_log_namespace),
            #[cfg(feature = "sources-statsd")]
            Self::Statsd(config) => config.outputs(global_log_namespace),
            #[cfg(feature = "sources-stdin")]
            Self::Stdin(config) => config.outputs(global_log_namespace),
            #[cfg(feature = "sources-syslog")]
            Self::Syslog(config) => config.outputs(global_log_namespace),
            Self::UnitTest(config) => config.outputs(global_log_namespace),
            #[cfg(feature = "sources-vector")]
            Self::Vector(config) => config.outputs(global_log_namespace),
        }
    }

    fn resources(&self) -> Vec<Resource> {
        match self {
            #[cfg(feature = "sources-apache_metrics")]
            Self::ApacheMetrics(config) => config.resources(),
            #[cfg(feature = "sources-aws_ecs_metrics")]
            Self::AwsEcsMetrics(config) => config.resources(),
            #[cfg(feature = "sources-aws_kinesis_firehose")]
            Self::AwsKinesisFirehose(config) => config.resources(),
            #[cfg(feature = "sources-aws_s3")]
            Self::AwsS3(config) => config.resources(),
            #[cfg(feature = "sources-aws_sqs")]
            Self::AwsSqs(config) => config.resources(),
            #[cfg(feature = "sources-datadog_agent")]
            Self::DatadogAgent(config) => config.resources(),
            #[cfg(feature = "sources-demo_logs")]
            Self::DemoLogs(config) => config.resources(),
            #[cfg(all(unix, feature = "sources-dnstap"))]
            Self::Dnstap(config) => config.resources(),
            #[cfg(feature = "sources-docker_logs")]
            Self::DockerLogs(config) => config.resources(),
            #[cfg(feature = "sources-eventstoredb_metrics")]
            Self::EventstoredbMetrics(config) => config.resources(),
            #[cfg(feature = "sources-exec")]
            Self::Exec(config) => config.resources(),
            #[cfg(feature = "sources-file")]
            Self::File(config) => config.resources(),
            #[cfg(all(unix, feature = "sources-file-descriptor"))]
            Self::FileDescriptor(config) => config.resources(),
            #[cfg(feature = "sources-fluent")]
            Self::Fluent(config) => config.resources(),
            #[cfg(feature = "sources-gcp_pubsub")]
            Self::GcpPubsub(config) => config.resources(),
            #[cfg(feature = "sources-heroku_logs")]
            Self::HerokuLogs(config) => config.resources(),
            #[cfg(feature = "sources-host_metrics")]
            Self::HostMetrics(config) => config.resources(),
            #[cfg(feature = "sources-http")]
            Self::Http(config) => config.resources(),
            #[cfg(feature = "sources-http_scrape")]
            Self::HttpScrape(config) => config.resources(),
            #[cfg(feature = "sources-internal_logs")]
            Self::InternalLogs(config) => config.resources(),
            #[cfg(feature = "sources-internal_metrics")]
            Self::InternalMetrics(config) => config.resources(),
            #[cfg(all(unix, feature = "sources-journald"))]
            Self::Journald(config) => config.resources(),
            #[cfg(feature = "sources-kafka")]
            Self::Kafka(config) => config.resources(),
            #[cfg(feature = "sources-kubernetes_logs")]
            Self::KubernetesLogs(config) => config.resources(),
            #[cfg(all(feature = "sources-logstash"))]
            Self::Logstash(config) => config.resources(),
            #[cfg(feature = "sources-mongodb_metrics")]
            Self::MongodbMetrics(config) => config.resources(),
            #[cfg(all(feature = "sources-nats"))]
            Self::Nats(config) => config.resources(),
            #[cfg(feature = "sources-nginx_metrics")]
            Self::NginxMetrics(config) => config.resources(),
            #[cfg(feature = "sources-opentelemetry")]
            Self::Opentelemetry(config) => config.resources(),
            #[cfg(feature = "sources-postgresql_metrics")]
            Self::PostgresqlMetrics(config) => config.resources(),
            #[cfg(feature = "sources-prometheus")]
            Self::PrometheusScrape(config) => config.resources(),
            #[cfg(feature = "sources-prometheus")]
            Self::PrometheusRemoteWrite(config) => config.resources(),
            #[cfg(feature = "sources-redis")]
            Self::Redis(config) => config.resources(),
            #[cfg(test)]
            Self::TestBackpressure(config) => config.resources(),
            #[cfg(test)]
            Self::TestBasic(config) => config.resources(),
            #[cfg(test)]
            Self::TestError(config) => config.resources(),
            #[cfg(test)]
            Self::TestPanic(config) => config.resources(),
            #[cfg(test)]
            Self::TestTripwire(config) => config.resources(),
            #[cfg(feature = "sources-socket")]
            Self::Socket(config) => config.resources(),
            #[cfg(feature = "sources-splunk_hec")]
            Self::SplunkHec(config) => config.resources(),
            #[cfg(feature = "sources-statsd")]
            Self::Statsd(config) => config.resources(),
            #[cfg(feature = "sources-stdin")]
            Self::Stdin(config) => config.resources(),
            #[cfg(feature = "sources-syslog")]
            Self::Syslog(config) => config.resources(),
            Self::UnitTest(config) => config.resources(),
            #[cfg(feature = "sources-vector")]
            Self::Vector(config) => config.resources(),
        }
    }

    fn can_acknowledge(&self) -> bool {
        match self {
            #[cfg(feature = "sources-apache_metrics")]
            Self::ApacheMetrics(config) => config.can_acknowledge(),
            #[cfg(feature = "sources-aws_ecs_metrics")]
            Self::AwsEcsMetrics(config) => config.can_acknowledge(),
            #[cfg(feature = "sources-aws_kinesis_firehose")]
            Self::AwsKinesisFirehose(config) => config.can_acknowledge(),
            #[cfg(feature = "sources-aws_s3")]
            Self::AwsS3(config) => config.can_acknowledge(),
            #[cfg(feature = "sources-aws_sqs")]
            Self::AwsSqs(config) => config.can_acknowledge(),
            #[cfg(feature = "sources-datadog_agent")]
            Self::DatadogAgent(config) => config.can_acknowledge(),
            #[cfg(feature = "sources-demo_logs")]
            Self::DemoLogs(config) => config.can_acknowledge(),
            #[cfg(all(unix, feature = "sources-dnstap"))]
            Self::Dnstap(config) => config.can_acknowledge(),
            #[cfg(feature = "sources-docker_logs")]
            Self::DockerLogs(config) => config.can_acknowledge(),
            #[cfg(feature = "sources-eventstoredb_metrics")]
            Self::EventstoredbMetrics(config) => config.can_acknowledge(),
            #[cfg(feature = "sources-exec")]
            Self::Exec(config) => config.can_acknowledge(),
            #[cfg(feature = "sources-file")]
            Self::File(config) => config.can_acknowledge(),
            #[cfg(all(unix, feature = "sources-file-descriptor"))]
            Self::FileDescriptor(config) => config.can_acknowledge(),
            #[cfg(feature = "sources-fluent")]
            Self::Fluent(config) => config.can_acknowledge(),
            #[cfg(feature = "sources-gcp_pubsub")]
            Self::GcpPubsub(config) => config.can_acknowledge(),
            #[cfg(feature = "sources-heroku_logs")]
            Self::HerokuLogs(config) => config.can_acknowledge(),
            #[cfg(feature = "sources-host_metrics")]
            Self::HostMetrics(config) => config.can_acknowledge(),
            #[cfg(feature = "sources-http")]
            Self::Http(config) => config.can_acknowledge(),
            #[cfg(feature = "sources-http_scrape")]
            Self::HttpScrape(config) => config.can_acknowledge(),
            #[cfg(feature = "sources-internal_logs")]
            Self::InternalLogs(config) => config.can_acknowledge(),
            #[cfg(feature = "sources-internal_metrics")]
            Self::InternalMetrics(config) => config.can_acknowledge(),
            #[cfg(all(unix, feature = "sources-journald"))]
            Self::Journald(config) => config.can_acknowledge(),
            #[cfg(feature = "sources-kafka")]
            Self::Kafka(config) => config.can_acknowledge(),
            #[cfg(feature = "sources-kubernetes_logs")]
            Self::KubernetesLogs(config) => config.can_acknowledge(),
            #[cfg(all(feature = "sources-logstash"))]
            Self::Logstash(config) => config.can_acknowledge(),
            #[cfg(feature = "sources-mongodb_metrics")]
            Self::MongodbMetrics(config) => config.can_acknowledge(),
            #[cfg(all(feature = "sources-nats"))]
            Self::Nats(config) => config.can_acknowledge(),
            #[cfg(feature = "sources-nginx_metrics")]
            Self::NginxMetrics(config) => config.can_acknowledge(),
            #[cfg(feature = "sources-opentelemetry")]
            Self::Opentelemetry(config) => config.can_acknowledge(),
            #[cfg(feature = "sources-postgresql_metrics")]
            Self::PostgresqlMetrics(config) => config.can_acknowledge(),
            #[cfg(feature = "sources-prometheus")]
            Self::PrometheusScrape(config) => config.can_acknowledge(),
            #[cfg(feature = "sources-prometheus")]
            Self::PrometheusRemoteWrite(config) => config.can_acknowledge(),
            #[cfg(feature = "sources-redis")]
            Self::Redis(config) => config.can_acknowledge(),
            #[cfg(test)]
            Self::TestBackpressure(config) => config.can_acknowledge(),
            #[cfg(test)]
            Self::TestBasic(config) => config.can_acknowledge(),
            #[cfg(test)]
            Self::TestError(config) => config.can_acknowledge(),
            #[cfg(test)]
            Self::TestPanic(config) => config.can_acknowledge(),
            #[cfg(test)]
            Self::TestTripwire(config) => config.can_acknowledge(),
            #[cfg(feature = "sources-socket")]
            Self::Socket(config) => config.can_acknowledge(),
            #[cfg(feature = "sources-splunk_hec")]
            Self::SplunkHec(config) => config.can_acknowledge(),
            #[cfg(feature = "sources-statsd")]
            Self::Statsd(config) => config.can_acknowledge(),
            #[cfg(feature = "sources-stdin")]
            Self::Stdin(config) => config.can_acknowledge(),
            #[cfg(feature = "sources-syslog")]
            Self::Syslog(config) => config.can_acknowledge(),
            Self::UnitTest(config) => config.can_acknowledge(),
            #[cfg(feature = "sources-vector")]
            Self::Vector(config) => config.can_acknowledge(),
        }
    }
}

impl NamedComponent for Sources {
    const NAME: &'static str = "_invalid_usage";

    fn get_component_name(&self) -> &'static str {
        match self {
            #[cfg(feature = "sources-apache_metrics")]
            Self::ApacheMetrics(config) => config.get_component_name(),
            #[cfg(feature = "sources-aws_ecs_metrics")]
            Self::AwsEcsMetrics(config) => config.get_component_name(),
            #[cfg(feature = "sources-aws_kinesis_firehose")]
            Self::AwsKinesisFirehose(config) => config.get_component_name(),
            #[cfg(feature = "sources-aws_s3")]
            Self::AwsS3(config) => config.get_component_name(),
            #[cfg(feature = "sources-aws_sqs")]
            Self::AwsSqs(config) => config.get_component_name(),
            #[cfg(feature = "sources-datadog_agent")]
            Self::DatadogAgent(config) => config.get_component_name(),
            #[cfg(feature = "sources-demo_logs")]
            Self::DemoLogs(config) => config.get_component_name(),
            #[cfg(all(unix, feature = "sources-dnstap"))]
            Self::Dnstap(config) => config.get_component_name(),
            #[cfg(feature = "sources-docker_logs")]
            Self::DockerLogs(config) => config.get_component_name(),
            #[cfg(feature = "sources-eventstoredb_metrics")]
            Self::EventstoredbMetrics(config) => config.get_component_name(),
            #[cfg(feature = "sources-exec")]
            Self::Exec(config) => config.get_component_name(),
            #[cfg(feature = "sources-file")]
            Self::File(config) => config.get_component_name(),
            #[cfg(all(unix, feature = "sources-file-descriptor"))]
            Self::FileDescriptor(config) => config.get_component_name(),
            #[cfg(feature = "sources-fluent")]
            Self::Fluent(config) => config.get_component_name(),
            #[cfg(feature = "sources-gcp_pubsub")]
            Self::GcpPubsub(config) => config.get_component_name(),
            #[cfg(feature = "sources-heroku_logs")]
            Self::HerokuLogs(config) => config.get_component_name(),
            #[cfg(feature = "sources-host_metrics")]
            Self::HostMetrics(config) => config.get_component_name(),
            #[cfg(feature = "sources-http")]
            Self::Http(config) => config.get_component_name(),
            #[cfg(feature = "sources-http_scrape")]
            Self::HttpScrape(config) => config.get_component_name(),
            #[cfg(feature = "sources-internal_logs")]
            Self::InternalLogs(config) => config.get_component_name(),
            #[cfg(feature = "sources-internal_metrics")]
            Self::InternalMetrics(config) => config.get_component_name(),
            #[cfg(all(unix, feature = "sources-journald"))]
            Self::Journald(config) => config.get_component_name(),
            #[cfg(feature = "sources-kafka")]
            Self::Kafka(config) => config.get_component_name(),
            #[cfg(feature = "sources-kubernetes_logs")]
            Self::KubernetesLogs(config) => config.get_component_name(),
            #[cfg(all(feature = "sources-logstash"))]
            Self::Logstash(config) => config.get_component_name(),
            #[cfg(feature = "sources-mongodb_metrics")]
            Self::MongodbMetrics(config) => config.get_component_name(),
            #[cfg(all(feature = "sources-nats"))]
            Self::Nats(config) => config.get_component_name(),
            #[cfg(feature = "sources-nginx_metrics")]
            Self::NginxMetrics(config) => config.get_component_name(),
            #[cfg(feature = "sources-opentelemetry")]
            Self::Opentelemetry(config) => config.get_component_name(),
            #[cfg(feature = "sources-postgresql_metrics")]
            Self::PostgresqlMetrics(config) => config.get_component_name(),
            #[cfg(feature = "sources-prometheus")]
            Self::PrometheusScrape(config) => config.get_component_name(),
            #[cfg(feature = "sources-prometheus")]
            Self::PrometheusRemoteWrite(config) => config.get_component_name(),
            #[cfg(feature = "sources-redis")]
            Self::Redis(config) => config.get_component_name(),
            #[cfg(test)]
            Self::TestBackpressure(config) => config.get_component_name(),
            #[cfg(test)]
            Self::TestBasic(config) => config.get_component_name(),
            #[cfg(test)]
            Self::TestError(config) => config.get_component_name(),
            #[cfg(test)]
            Self::TestPanic(config) => config.get_component_name(),
            #[cfg(test)]
            Self::TestTripwire(config) => config.get_component_name(),
            #[cfg(feature = "sources-socket")]
            Self::Socket(config) => config.get_component_name(),
            #[cfg(feature = "sources-splunk_hec")]
            Self::SplunkHec(config) => config.get_component_name(),
            #[cfg(feature = "sources-statsd")]
            Self::Statsd(config) => config.get_component_name(),
            #[cfg(feature = "sources-stdin")]
            Self::Stdin(config) => config.get_component_name(),
            #[cfg(feature = "sources-syslog")]
            Self::Syslog(config) => config.get_component_name(),
            Self::UnitTest(config) => config.get_component_name(),
            #[cfg(feature = "sources-vector")]
            Self::Vector(config) => config.get_component_name(),
        }
    }
}

#[cfg(test)]
mod tests {
    use vector_config::{configurable_component, schema::generate_root_schema};

    use crate::sources::Sources;

    /// Top-level Vector configuration. (mock)
    #[configurable_component]
    #[derive(Clone)]
    struct MockRootConfig {
        /// All configured sources.
        sources: Vec<Sources>,
    }

    #[test]
    #[ignore]
    #[allow(clippy::print_stdout)]
    fn vector_config() {
        let root_schema = generate_root_schema::<MockRootConfig>();
        let json = serde_json::to_string_pretty(&root_schema)
            .expect("rendering root schema to JSON should not fail");

        println!("{}", json);
    }
}
