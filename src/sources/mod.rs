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
    /// Collect events from AMQP 0.9.1 compatible brokers like RabbitMQ.
    #[cfg(feature = "sources-amqp")]
    #[configurable(metadata(docs::label = "AMQP"))]
    Amqp(amqp::AmqpSourceConfig),

    /// Collect metrics from Apache's HTTPD server.
    #[cfg(feature = "sources-apache_metrics")]
    #[configurable(metadata(docs::label = "Apache Metrics"))]
    ApacheMetrics(apache_metrics::ApacheMetricsConfig),

    /// Collect Docker container stats for tasks running in AWS ECS and AWS Fargate.
    #[cfg(feature = "sources-aws_ecs_metrics")]
    #[configurable(metadata(docs::label = "AWS ECS Metrics"))]
    AwsEcsMetrics(aws_ecs_metrics::AwsEcsMetricsSourceConfig),

    /// Collect logs from AWS Kinesis Firehose.
    #[cfg(feature = "sources-aws_kinesis_firehose")]
    #[configurable(metadata(docs::label = "AWS Kinesis Firehose"))]
    AwsKinesisFirehose(aws_kinesis_firehose::AwsKinesisFirehoseConfig),

    /// Collect logs from AWS S3.
    #[cfg(feature = "sources-aws_s3")]
    #[configurable(metadata(docs::label = "AWS S3"))]
    AwsS3(aws_s3::AwsS3Config),

    /// Collect logs from AWS SQS.
    #[cfg(feature = "sources-aws_sqs")]
    #[configurable(metadata(docs::label = "AWS SQS"))]
    AwsSqs(aws_sqs::AwsSqsConfig),

    /// Receive logs, metrics, and traces collected by a Datadog Agent.
    #[cfg(feature = "sources-datadog_agent")]
    #[configurable(metadata(docs::label = "Datadog Agent"))]
    DatadogAgent(datadog_agent::DatadogAgentConfig),

    /// Generate fake log events, which can be useful for testing and demos.
    #[cfg(feature = "sources-demo_logs")]
    #[configurable(metadata(docs::label = "Demo Logs"))]
    DemoLogs(demo_logs::DemoLogsConfig),

    /// Collect DNS logs from a dnstap-compatible server.
    #[cfg(all(unix, feature = "sources-dnstap"))]
    #[configurable(metadata(docs::label = "dnstap"))]
    Dnstap(dnstap::DnstapConfig),

    /// Collect container logs from a Docker Daemon.
    #[cfg(feature = "sources-docker_logs")]
    #[configurable(metadata(docs::label = "Docker Logs"))]
    DockerLogs(docker_logs::DockerLogsConfig),

    /// Receive metrics from collected by a EventStoreDB.
    #[cfg(feature = "sources-eventstoredb_metrics")]
    #[configurable(metadata(docs::label = "EventStoreDB Metrics"))]
    EventstoredbMetrics(eventstoredb_metrics::EventStoreDbConfig),

    /// Collect output from a process running on the host.
    #[cfg(feature = "sources-exec")]
    #[configurable(metadata(docs::label = "Exec"))]
    Exec(exec::ExecConfig),

    /// Collect logs from files.
    #[cfg(feature = "sources-file")]
    #[configurable(metadata(docs::label = "File"))]
    File(file::FileConfig),

    /// Collect logs from a file descriptor.
    #[cfg(all(unix, feature = "sources-file-descriptor"))]
    #[configurable(metadata(docs::label = "File Descriptor"))]
    FileDescriptor(file_descriptors::file_descriptor::FileDescriptorSourceConfig),

    /// Collect logs from a Fluentd or Fluent Bit agent.
    #[cfg(feature = "sources-fluent")]
    #[configurable(metadata(docs::label = "Fluent"))]
    Fluent(fluent::FluentConfig),

    /// Fetch observability events from GCP's Pub/Sub messaging system.
    #[cfg(feature = "sources-gcp_pubsub")]
    #[configurable(metadata(docs::label = "GCP Pub/Sub"))]
    GcpPubsub(gcp_pubsub::PubsubConfig),

    /// Collect logs from Heroku's Logplex, the router responsible for receiving logs from your Heroku apps.
    #[cfg(feature = "sources-heroku_logs")]
    #[configurable(metadata(docs::label = "Heroku Logplex"))]
    HerokuLogs(heroku_logs::LogplexConfig),

    /// Collect metric data from the local system.
    #[cfg(feature = "sources-host_metrics")]
    #[configurable(metadata(docs::label = "Host metrics"))]
    HostMetrics(host_metrics::HostMetricsConfig),

    /// Host an HTTP endpoint to receive logs.
    #[cfg(feature = "sources-http_server")]
    #[configurable(deprecated)]
    #[configurable(metadata(docs::label = "HTTP"))]
    Http(http_server::HttpConfig),

    /// Pull observability data from an HTTP server at a configured interval.
    #[cfg(feature = "sources-http_client")]
    #[configurable(metadata(docs::label = "HTTP Client"))]
    HttpClient(http_client::HttpClientConfig),

    /// Host an HTTP endpoint to receive logs.
    #[cfg(feature = "sources-http_server")]
    #[configurable(metadata(docs::label = "HTTP Server"))]
    HttpServer(http_server::SimpleHttpConfig),

    /// Expose internal log messages emitted by the running Vector instance.
    #[cfg(feature = "sources-internal_logs")]
    #[configurable(metadata(docs::label = "Internal Logs"))]
    InternalLogs(internal_logs::InternalLogsConfig),

    /// Expose internal metrics emitted by the running Vector instance.
    #[cfg(feature = "sources-internal_metrics")]
    #[configurable(metadata(docs::label = "Internal Metrics"))]
    InternalMetrics(internal_metrics::InternalMetricsConfig),

    /// Collect logs from JournalD.
    #[cfg(all(unix, feature = "sources-journald"))]
    #[configurable(metadata(docs::label = "JournalD"))]
    Journald(journald::JournaldConfig),

    /// Collect logs from Apache Kafka.
    #[cfg(feature = "sources-kafka")]
    #[configurable(metadata(docs::label = "Kafka"))]
    Kafka(kafka::KafkaSourceConfig),

    /// Collect Pod logs from Kubernetes Nodes.
    #[cfg(feature = "sources-kubernetes_logs")]
    #[configurable(metadata(docs::label = "Kubernetes Logs"))]
    KubernetesLogs(kubernetes_logs::Config),

    /// Collect logs from a Logstash agent.
    #[cfg(all(feature = "sources-logstash"))]
    #[configurable(metadata(docs::label = "Logstash"))]
    Logstash(logstash::LogstashConfig),

    /// Collect metrics from the MongoDB database.
    #[cfg(feature = "sources-mongodb_metrics")]
    #[configurable(metadata(docs::label = "MongoDB Metrics"))]
    MongodbMetrics(mongodb_metrics::MongoDbMetricsConfig),

    /// Read observability data from subjects on the NATS messaging system.
    #[cfg(all(feature = "sources-nats"))]
    #[configurable(metadata(docs::label = "NATS"))]
    Nats(nats::NatsSourceConfig),

    /// Collect metrics from NGINX.
    #[cfg(feature = "sources-nginx_metrics")]
    #[configurable(metadata(docs::label = "NGINX"))]
    NginxMetrics(nginx_metrics::NginxMetricsConfig),

    /// Receive OTLP data through gRPC or HTTP.
    #[cfg(feature = "sources-opentelemetry")]
    #[configurable(metadata(docs::label = "OpenTelemetry"))]
    Opentelemetry(opentelemetry::OpentelemetryConfig),

    /// Collect metrics from the PostgreSQL database.
    #[cfg(feature = "sources-postgresql_metrics")]
    #[configurable(metadata(docs::label = "PostgreSQL Metrics"))]
    PostgresqlMetrics(postgresql_metrics::PostgresqlMetricsConfig),

    /// Collect metrics from Prometheus exporters.
    #[cfg(feature = "sources-prometheus")]
    #[configurable(metadata(docs::label = "Prometheus Scrape"))]
    PrometheusScrape(prometheus::PrometheusScrapeConfig),

    /// Receive metric via the Prometheus Remote Write protocol.
    #[cfg(feature = "sources-prometheus")]
    #[configurable(metadata(docs::label = "Prometheus Remote Write"))]
    PrometheusRemoteWrite(prometheus::PrometheusRemoteWriteConfig),

    /// Collect observability data from Redis.
    #[cfg(feature = "sources-redis")]
    #[configurable(metadata(docs::label = "Redis"))]
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

    /// Collect logs over a socket.
    #[cfg(feature = "sources-socket")]
    #[configurable(metadata(docs::label = "Socket"))]
    Socket(socket::SocketConfig),

    /// Receive logs from Splunk.
    #[cfg(feature = "sources-splunk_hec")]
    #[configurable(metadata(docs::label = "Splunk HEC"))]
    SplunkHec(splunk_hec::SplunkConfig),

    /// Collect metrics emitted by the StatsD aggregator.
    #[cfg(feature = "sources-statsd")]
    #[configurable(metadata(docs::label = "StatsD"))]
    Statsd(statsd::StatsdConfig),

    /// Collect logs sent via stdin.
    #[cfg(feature = "sources-stdin")]
    #[configurable(metadata(docs::label = "stdin"))]
    Stdin(file_descriptors::stdin::StdinConfig),

    /// Collect logs sent via Syslog.
    #[cfg(feature = "sources-syslog")]
    #[configurable(metadata(docs::label = "Syslog"))]
    Syslog(syslog::SyslogConfig),

    /// Unit test.
    UnitTest(UnitTestSourceConfig),

    /// Unit test stream.
    UnitTestStream(UnitTestStreamSourceConfig),

    /// Collect observability data from a Vector instance.
    #[cfg(feature = "sources-vector")]
    #[configurable(metadata(docs::label = "Vector"))]
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
