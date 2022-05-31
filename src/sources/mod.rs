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

#[configurable_component]
#[derive(Clone)]
pub enum Sources {
    /// Apache HTTP Server (HTTPD) Metrics.
    #[cfg(feature = "sources-apache_metrics")]
    ApacheMetrics(#[configurable(derived)] apache_metrics::ApacheMetricsConfig),
    /// AWS ECS Metrics.
    #[cfg(feature = "sources-aws_ecs_metrics")]
    AwsEcsMetrics(#[configurable(derived)] aws_ecs_metrics::AwsEcsMetricsSourceConfig),
    /*#[cfg(feature = "sources-aws_kinesis_firehose")]
    AwsKinesisFirehose(#[configurable(derived)] aws_kinesis_firehose::AwsKinesisFirehoseConfig),
    #[cfg(feature = "sources-aws_s3")]
    AwsS3(#[configurable(derived)] aws_s3::AwsS3Config),
    #[cfg(feature = "sources-aws_sqs")]
    AwsSqs(#[configurable(derived)] aws_sqs::AwsSqsConfig),
    #[cfg(feature = "sources-datadog_agent")]
    DatadogAgent(#[configurable(derived)] datadog::agent::DatadogAgentConfig),
    #[cfg(feature = "sources-demo_logs")]
    DemoLogs(#[configurable(derived)] demo_logs::DemoLogsConfig),
    #[cfg(all(unix, feature = "sources-dnstap"))]
    Dnstap(#[configurable(derived)] dnstap::DnstapConfig),
    #[cfg(feature = "sources-docker_logs")]
    DockerLogs(#[configurable(derived)] docker_logs::DockerLogsConfig),
    #[cfg(feature = "sources-eventstoredb_metrics")]
    EventstoreDbMetrics(#[configurable(derived)] eventstoredb_metrics::EventStoreDbConfig),
    #[cfg(feature = "sources-exec")]
    Exec(#[configurable(derived)] exec::ExecConfig),
    #[cfg(feature = "sources-file")]
    File(#[configurable(derived)] file::FileConfig),
    #[cfg(feature = "sources-fluent")]
    Fluent(#[configurable(derived)] fluent::FluentConfig),
    #[cfg(feature = "sources-gcp_pubsub")]
    GcpPubsub(#[configurable(derived)] gcp_pubsub::PubsubConfig),
    #[cfg(feature = "sources-heroku_logs")]
    HerokuLogs(#[configurable(derived)] heroku_logs::LogplexConfig),
    #[cfg(feature = "sources-host_metrics")]
    HostMetrics(#[configurable(derived)] host_metrics::HostMetricsConfig),
    #[cfg(feature = "sources-http")]
    Http(#[configurable(derived)] http::SimpleHttpConfig),
    #[cfg(feature = "sources-internal_logs")]
    InternalLogs(#[configurable(derived)] internal_logs::InternalLogsConfig),
    #[cfg(feature = "sources-internal_metrics")]
    InternalMetrics(#[configurable(derived)] internal_metrics::InternalMetricsConfig),
    #[cfg(all(unix, feature = "sources-journald"))]
    Journald(#[configurable(derived)] journald::JournaldConfig),
    #[cfg(all(feature = "sources-kafka", feature = "rdkafka"))]
    Kafka(#[configurable(derived)] kafka::KafkaSourceConfig),
    #[cfg(feature = "sources-kubernetes_logs")]
    KubernetesLogs(#[configurable(derived)] kubernetes_logs::Config),
    #[cfg(all(feature = "sources-logstash"))]
    Logstash(#[configurable(derived)] logstash::LogstashConfig),
    #[cfg(feature = "sources-mongodb_metrics")]
    MongodbMetrics(#[configurable(derived)] mongodb_metrics::MongoDbMetricsConfig),
    #[cfg(all(feature = "sources-nats"))]
    Nats(#[configurable(derived)] nats::NatsSourceConfig),
    #[cfg(feature = "sources-nginx_metrics")]
    NginxMetrics(#[configurable(derived)] nginx_metrics::NginxMetricsConfig),
    #[cfg(feature = "sources-postgresql_metrics")]
    PostgresqlMetrics(#[configurable(derived)] postgresql_metrics::PostgresqlMetricsConfig),
    #[cfg(feature = "sources-prometheus")]
    PrometheusScrape(#[configurable(derived)] prometheus::scrape::PrometheusScrapeConfig),
    #[cfg(feature = "sources-prometheus")]
    PrometheusRemoteWrite(#[configurable(derived)] prometheus::remote_write::PrometheusRemoteWriteConfig),
    #[cfg(feature = "sources-redis")]
    Redis(#[configurable(derived)] redis::RedisSourceConfig),
    #[cfg(feature = "sources-socket")]
    Socket(#[configurable(derived)] socket::SocketConfig),
    #[cfg(feature = "sources-splunk_hec")]
    SplunkHec(#[configurable(derived)] splunk_hec::SplunkConfig),
    #[cfg(feature = "sources-statsd")]
    Statsd(#[configurable(derived)] statsd::StatsdConfig),
    #[cfg(feature = "sources-stdin")]
    Stdin(#[configurable(derived)] stdin::StdinConfig),
    #[cfg(feature = "sources-syslog")]
    Syslog(#[configurable(derived)] syslog::SyslogConfig),
    #[cfg(feature = "sources-vector")]
    Vector(#[configurable(derived)] vector::VectorConfig),*/
}
