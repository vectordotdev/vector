package metadata

#Guide: {
	#Str: !=""
	#EventType: "logs" | "metrics" | "logs and metrics"

	source?: #Str
	sink?: #Str
	platform?: "docker" | "kubernetes"
	event_type: #EventType

	component_name: #Str

	if platform != _|_ {
		if platform == "docker" {
			component_name: "docker_logs"
		}

		if platform == "kubernetes" {
			component_name: "kubernetes_logs"
		}
	}

	if source != _|_ {
		component_name: source
	}

	if sink != _|_ {
		component_name: sink
	}

	if source != _|_ || platform != _|_ {
		if event_type == "logs" {
			sinks: _logs_sinks
		}

		if event_type == "metrics" {
			sinks: _metrics_sinks
		}
	}
}

_metrics_sinks: [
	"aws_cloudwatch_metrics",
	"datadog_metrics",
	"gcp_stackdriver_metrics",
	"humio_metrics",
	"influxdb_metrics",
	"kafka",
	"prometheus_remote_write",
	"sematext_metrics",
	"statsd",
]

_logs_sinks: [
	"aws_cloudwatch_logs",
	"aws_kinesis_firehose",
	"aws_kinesis_streams",
	"aws_s3",
	"aws_sqs",
	"azure_monitor_logs",
	"clickhouse",
	"datadog_logs",
	"elasticsearch",
	"file",
	"gcp_cloud_storage",
	"gcp_stackdriver_logs",
	"gcp_pubsub",
	"honeycomb",
	"http",
	"humio_logs",
	"influxdb_logs",
	"kafka",
	"logdna",
	"loki",
	"nats",
	"new_relic_logs",
	"papertrail",
	"pulsar",
	"sematext_logs",
	"socket",
	"splunk_hec",
]

guides: integrate: [#Guide, ...#Guide] & [
	{
		sink: "aws_sqs"
		event_type: "logs"
	},
	{
		source: "apache_metrics"
		event_type: "metrics"
	},
	{
		sink: "pulsar"
		event_type: "logs"
	},
	{
		sink: "aws_cloudwatch_metrics"
		event_type: "metrics"
	},
	{
		sink: "aws_ecs_metrics"
		event_type: "metrics"
	},
	{
		source: "aws_ecs_metrics"
		event_type: "metrics"
	},
	{
		sink: "aws_kinesis_firehose"
		event_type: "logs"
	},
	{
		source: "aws_kinesis_firehose"
		event_type: "logs"
	},
	{
		sink: "aws_kinesis_streams"
		event_type: "logs"
	},
	{
		sink: "aws_s3"
		event_type: "logs"
	},
	{
		source: "aws_s3"
		event_type: "logs"
	},
	{
		sink: "azure_monitor_logs"
		event_type: "logs"
	},
	{
		sink: "clickhouse"
		event_type: "logs"
	},
	{
		sink: "datadog_logs"
		event_type: "logs"
	},
	{
		source: "datadog_logs"
		event_type: "logs"
	},
	{
		sink: "datadog_metrics"
		event_type: "metrics"
	},
	{
		platform: "docker"
		event_type: "logs"
	},
	{
		sink: "elasticsearch"
		event_type: "logs"
	},
	{
		source: "exec"
		event_type: "logs"
	},
	{
		sink: "file"
		event_type: "logs"
	},
	{
		source: "file"
		event_type: "logs"
	},
	{
		sink: "gcp_stackdriver_metrics"
		event_type: "metrics"
	},
	{
		sink: "gcp_cloud_storage"
		event_type: "logs"
	},
	{
		sink: "gcp_stackdriver_logs"
		event_type: "logs"
	},
	{
		sink: "gcp_pubsub"
		event_type: "logs"
	},
	{
		source: "heroku_logs"
		event_type: "logs"
	},
	{
		sink: "honeycomb"
		event_type: "logs"
	},
	{
		source: "host_metrics"
		event_type: "metrics"
	},
	{
		sink: "http"
		event_type: "logs"
	},
	{
		source: "http"
		event_type: "logs"
	},
	{
		sink: "humio_logs"
		event_type: "logs"
	},
	{
		sink: "humio_metrics"
		event_type: "metrics"
	},
	{
		sink: "influxdb_logs"
		event_type: "logs"
	},
	{
		sink: "influxdb_metrics"
		event_type: "metrics"
	},
	{
		source: "journald"
		event_type: "logs"
	},
	{
		sink: "kafka"
		event_type: "logs and metrics"
	},
	{
		source: "kafka"
		event_type: "logs"
	},
	{
		platform: "kubernetes"
		event_type: "logs"
	},
	{
		sink: "logdna"
		event_type: "logs"
	},
	{
		sink: "loki"
		event_type: "logs"
	},
	{
		source: "mongodb_metrics"
		event_type: "metrics"
	},
	{
		sink: "nats"
		event_type: "logs"
	},
	{
		sink: "new_relic_logs"
		event_type: "logs"
	},
	{
		source: "nginx_metrics"
		event_type: "metrics"
	},
	{
		sink: "papertrail"
		event_type: "logs"
	},
	{
		source: "postgresql_metrics"
		event_type: "metrics"
	},
	{
		source: "prometheus_scrape"
		event_type: "metrics"
	},
	{
		sink: "sematext_logs"
		event_type: "logs"
	},
	{
		sink: "sematext_metrics"
		event_type: "metrics"
	},
	{
		sink: "socket"
		event_type: "logs"
	},
	{
		sink: "splunk_hec"
		event_type: "logs"
	},
	{
		source: "splunk_hec"
		event_type: "logs"
	},
	{
		sink: "statsd"
		event_type: "metrics"
	},
	{
		source: "statsd"
		event_type: "metrics"
	},
	{
		source: "stdin"
		event_type: "logs"
	},
	{
		source: "syslog"
		event_type: "logs"
	}
]
