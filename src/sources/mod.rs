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
pub mod datadog;
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
#[cfg(feature = "sources-internal_logs")]
pub mod internal_logs;
#[cfg(feature = "sources-internal_metrics")]
pub mod internal_metrics;
#[cfg(all(unix, feature = "sources-journald"))]
pub mod journald;
#[cfg(all(feature = "sources-kafka", feature = "rdkafka"))]
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
#[cfg(feature = "sources-stdin")]
pub mod stdin;
#[cfg(feature = "sources-syslog")]
pub mod syslog;
#[cfg(feature = "sources-vector")]
pub mod vector;

pub(crate) mod util;

use vector_config::configurable_component;
pub use vector_core::source::Source;

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
    DatadogAgent(#[configurable(derived)] datadog::agent::DatadogAgentConfig),

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
    EventstoreDbMetrics(#[configurable(derived)] eventstoredb_metrics::EventStoreDbConfig),

    /// Exec.
    #[cfg(feature = "sources-exec")]
    Exec(#[configurable(derived)] exec::ExecConfig),

    /// File.
    #[cfg(feature = "sources-file")]
    File(#[configurable(derived)] file::FileConfig),

    /// Fluent.
    #[cfg(feature = "sources-fluent")]
    Fluent(#[configurable(derived)] fluent::FluentConfig),

    /// GCP Pub/Sub.
    #[cfg(feature = "sources-gcp_pubsub")]
    GcpPubsub(#[configurable(derived)] gcp_pubsub::PubsubConfig),

    /// Generator.
    #[cfg(feature = "sources-demo_logs")]
    Generator(#[configurable(derived)] demo_logs::DemoLogsCompatConfig),

    /// Heroku Logs.
    #[cfg(feature = "sources-heroku_logs")]
    HerokuLogs(#[configurable(derived)] heroku_logs::LogplexConfig),

    /// Host Metrics.
    #[cfg(feature = "sources-host_metrics")]
    HostMetrics(#[configurable(derived)] host_metrics::HostMetricsConfig),

    /// HTTP.
    #[cfg(feature = "sources-http")]
    Http(#[configurable(derived)] http::SimpleHttpConfig),

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
    #[cfg(all(feature = "sources-kafka", feature = "rdkafka"))]
    Kafka(#[configurable(derived)] kafka::KafkaSourceConfig),

    /// Kubernetes Logs.
    #[cfg(feature = "sources-kubernetes_logs")]
    KubernetesLogs(#[configurable(derived)] kubernetes_logs::Config),

    /// Heroku Logs.
    #[cfg(feature = "sources-heroku_logs")]
    Logplex(#[configurable(derived)] heroku_logs::LogplexCompatConfig),

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
    Stdin(#[configurable(derived)] stdin::StdinConfig),

    /// Syslog.
    #[cfg(feature = "sources-syslog")]
    Syslog(#[configurable(derived)] syslog::SyslogConfig),

    /// Vector.
    #[cfg(feature = "sources-vector")]
    Vector(#[configurable(derived)] vector::VectorConfig),
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
