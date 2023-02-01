use enum_dispatch::enum_dispatch;
use snafu::Snafu;

#[cfg(feature = "sources-amqp")]
pub mod amqp;
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
#[cfg(feature = "sources-http_client")]
pub mod http_client;
#[cfg(feature = "sources-http_server")]
pub mod http_server;
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

pub mod util;

use vector_config::{configurable_component, NamedComponent};
use vector_core::config::{LogNamespace, Output};
pub use vector_core::source::Source;

use crate::config::{
    unit_test::{UnitTestSourceConfig, UnitTestStreamSourceConfig},
    Resource, SourceConfig, SourceContext,
};

/// Common build errors
#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("URI parse error: {}", source))]
    UriParseError { source: ::http::uri::InvalidUri },
}

/// Configurable sources in Vector.
#[allow(clippy::large_enum_variant)]
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
#[enum_dispatch(SourceConfig)]
pub enum Sources {
    /// AMQP.
    #[cfg(feature = "sources-amqp")]
    Amqp(amqp::AmqpSourceConfig),

    /// Apache HTTP Server (HTTPD) Metrics.
    #[cfg(feature = "sources-apache_metrics")]
    ApacheMetrics(apache_metrics::ApacheMetricsConfig),

    /// AWS ECS Metrics.
    #[cfg(feature = "sources-aws_ecs_metrics")]
    AwsEcsMetrics(aws_ecs_metrics::AwsEcsMetricsSourceConfig),

    /// AWS Kinesis Firehose.
    #[cfg(feature = "sources-aws_kinesis_firehose")]
    AwsKinesisFirehose(aws_kinesis_firehose::AwsKinesisFirehoseConfig),

    /// AWS S3.
    #[cfg(feature = "sources-aws_s3")]
    AwsS3(aws_s3::AwsS3Config),

    /// AWS SQS.
    #[cfg(feature = "sources-aws_sqs")]
    AwsSqs(aws_sqs::AwsSqsConfig),

    /// Datadog Agent.
    #[cfg(feature = "sources-datadog_agent")]
    DatadogAgent(datadog_agent::DatadogAgentConfig),

    /// Demo logs.
    #[cfg(feature = "sources-demo_logs")]
    DemoLogs(demo_logs::DemoLogsConfig),

    /// DNSTAP.
    #[cfg(all(unix, feature = "sources-dnstap"))]
    Dnstap(dnstap::DnstapConfig),

    /// Docker Logs.
    #[cfg(feature = "sources-docker_logs")]
    DockerLogs(docker_logs::DockerLogsConfig),

    /// EventStoreDB Metrics.
    #[cfg(feature = "sources-eventstoredb_metrics")]
    EventstoredbMetrics(eventstoredb_metrics::EventStoreDbConfig),

    /// Exec.
    #[cfg(feature = "sources-exec")]
    Exec(exec::ExecConfig),

    /// File.
    #[cfg(feature = "sources-file")]
    File(file::FileConfig),

    /// File descriptor.
    #[cfg(all(unix, feature = "sources-file-descriptor"))]
    FileDescriptor(file_descriptors::file_descriptor::FileDescriptorSourceConfig),

    /// Fluent.
    #[cfg(feature = "sources-fluent")]
    Fluent(fluent::FluentConfig),

    /// GCP Pub/Sub.
    #[cfg(feature = "sources-gcp_pubsub")]
    GcpPubsub(gcp_pubsub::PubsubConfig),

    /// Heroku Logs.
    #[cfg(feature = "sources-heroku_logs")]
    HerokuLogs(heroku_logs::LogplexConfig),

    /// Host Metrics.
    #[cfg(feature = "sources-host_metrics")]
    HostMetrics(host_metrics::HostMetricsConfig),

    /// HTTP.
    #[cfg(feature = "sources-http_server")]
    Http(http_server::HttpConfig),

    /// HTTP Client.
    #[cfg(feature = "sources-http_client")]
    HttpClient(http_client::HttpClientConfig),

    /// HTTP Server.
    #[cfg(feature = "sources-http_server")]
    HttpServer(http_server::SimpleHttpConfig),

    /// Internal Logs.
    #[cfg(feature = "sources-internal_logs")]
    InternalLogs(internal_logs::InternalLogsConfig),

    /// Internal Metrics.
    #[cfg(feature = "sources-internal_metrics")]
    InternalMetrics(internal_metrics::InternalMetricsConfig),

    /// Journald.
    #[cfg(all(unix, feature = "sources-journald"))]
    Journald(journald::JournaldConfig),

    /// Kafka.
    #[cfg(feature = "sources-kafka")]
    Kafka(kafka::KafkaSourceConfig),

    /// Kubernetes Logs.
    #[cfg(feature = "sources-kubernetes_logs")]
    KubernetesLogs(kubernetes_logs::Config),

    /// Logstash.
    #[cfg(all(feature = "sources-logstash"))]
    Logstash(logstash::LogstashConfig),

    /// MongoDB Metrics.
    #[cfg(feature = "sources-mongodb_metrics")]
    MongodbMetrics(mongodb_metrics::MongoDbMetricsConfig),

    /// NATS.
    #[cfg(all(feature = "sources-nats"))]
    Nats(nats::NatsSourceConfig),

    /// NGINX Metrics.
    #[cfg(feature = "sources-nginx_metrics")]
    NginxMetrics(nginx_metrics::NginxMetricsConfig),

    /// OpenTelemetry.
    #[cfg(feature = "sources-opentelemetry")]
    Opentelemetry(opentelemetry::OpentelemetryConfig),

    /// PostgreSQL Metrics.
    #[cfg(feature = "sources-postgresql_metrics")]
    PostgresqlMetrics(postgresql_metrics::PostgresqlMetricsConfig),

    /// Prometheus Scrape.
    #[cfg(feature = "sources-prometheus")]
    PrometheusScrape(prometheus::PrometheusScrapeConfig),

    /// Prometheus Remote Write.
    #[cfg(feature = "sources-prometheus")]
    PrometheusRemoteWrite(prometheus::PrometheusRemoteWriteConfig),

    /// Redis.
    #[cfg(feature = "sources-redis")]
    Redis(redis::RedisSourceConfig),

    /// Test (backpressure).
    #[cfg(test)]
    TestBackpressure(crate::test_util::mock::sources::BackpressureSourceConfig),

    /// Test (basic).
    #[cfg(test)]
    TestBasic(crate::test_util::mock::sources::BasicSourceConfig),

    /// Test (error).
    #[cfg(test)]
    TestError(crate::test_util::mock::sources::ErrorSourceConfig),

    /// Test (panic).
    #[cfg(test)]
    TestPanic(crate::test_util::mock::sources::PanicSourceConfig),

    /// Test (tripwire).
    #[cfg(test)]
    TestTripwire(crate::test_util::mock::sources::TripwireSourceConfig),

    /// Socket.
    #[cfg(feature = "sources-socket")]
    Socket(socket::SocketConfig),

    /// Splunk HEC.
    #[cfg(feature = "sources-splunk_hec")]
    SplunkHec(splunk_hec::SplunkConfig),

    /// StatsD.
    #[cfg(feature = "sources-statsd")]
    Statsd(statsd::StatsdConfig),

    /// Stdin.
    #[cfg(feature = "sources-stdin")]
    Stdin(file_descriptors::stdin::StdinConfig),

    /// Syslog.
    #[cfg(feature = "sources-syslog")]
    Syslog(syslog::SyslogConfig),

    /// Unit test.
    UnitTest(UnitTestSourceConfig),

    /// Unit test stream.
    UnitTestStream(UnitTestStreamSourceConfig),

    /// Vector.
    #[cfg(feature = "sources-vector")]
    Vector(vector::VectorConfig),
}

// We can't use `enum_dispatch` here because it doesn't support associated constants.
impl NamedComponent for Sources {
    const NAME: &'static str = "_invalid_usage";

    fn get_component_name(&self) -> &'static str {
        match self {
            #[cfg(feature = "sources-amqp")]
            Self::Amqp(config) => config.get_component_name(),
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
            #[cfg(feature = "sources-http_server")]
            Self::Http(config) => config.get_component_name(),
            #[cfg(feature = "sources-http_client")]
            Self::HttpClient(config) => config.get_component_name(),
            #[cfg(feature = "sources-http_server")]
            Self::HttpServer(config) => config.get_component_name(),
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
            Self::UnitTestStream(config) => config.get_component_name(),
            #[cfg(feature = "sources-vector")]
            Self::Vector(config) => config.get_component_name(),
        }
    }
}
