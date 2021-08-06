package metadata

#Guide: {
	#Str:       !=""
	#EventType: "logs" | "metrics" | "logs and metrics"

	source?:    #Str
	sink?:      #Str
	platform?:  "docker" | "kubernetes"
	service:    #Str
	event_type: #EventType

	component_name: #Str

	#Sink: {
		name:    #Str
		service: #Str

		if service == _|_ {
			service: name
		}
	}

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

	_logs_sinks: [#Sink, ...#Sink] & [
			{name: "aws_cloudwatch_logs"},
			{name: "aws_kinesis_firehose"},
			{name: "aws_kinesis_streams", service: "aws_kinesis_data_streams"},
			{name: "aws_s3"},
			{name: "aws_sqs"},
			{name: "azure_monitor_logs"},
			{name: "clickhouse"},
			{name: "datadog_logs"},
			{name: "elasticsearch"},
			{name: "file", service: "files"},
			{name: "gcp_cloud_storage"},
			{name: "gcp_stackdriver_logs", service: "gcp_cloud_monitoring"},
			{name: "gcp_pubsub"},
			{name: "honeycomb"},
			{name: "http"},
			{name: "humio_logs", service:    "humio"},
			{name: "influxdb_logs", service: "influxdb"},
			{name: "kafka"},
			{name: "logdna"},
			{name: "loki"},
			{name: "nats"},
			{name: "new_relic_logs"},
			{name: "papertrail"},
			{name: "pulsar"},
			{name: "sematext_logs", service: "sematext"},
			{name: "socket", service:        "socket_client"},
			{name: "splunk_hec", service:    "splunk"},
	]

	_metrics_sinks: [#Sink, ...#Sink] & [
			{name: "aws_cloudwatch_metrics"},
			{name: "datadog_metrics"},
			{name: "gcp_stackdriver_metrics", service: "gcp_cloud_monitoring"},
			{name: "humio_metrics", service:           "humio"},
			{name: "influxdb_metrics", service:        "influxdb"},
			{name: "kafka"},
			{name: "prometheus_remote_write"},
			{name: "sematext_metrics", service: "sematext"},
			{name: "statsd"},
	]
}

guides: types: {
	logs: """
		Logs are an essential part...
		"""

	metrics: """
		Metrics are an essential part...
		"""

	"logs and metrics": """
		Both logs and metrics...
		"""
}

guides: integrate: [#Guide, ...#Guide] & [
			{
		sink:       "aws_sqs"
		service:    "aws_sqs"
		event_type: "logs"
	},
	{
		source:     "apache_metrics"
		service:    "apache_http"
		event_type: "metrics"
	},
	{
		sink:       "pulsar"
		service:    "pulsar"
		event_type: "logs"
	},
	{
		sink:       "aws_cloudwatch_metrics"
		service:    "aws_cloudwatch_metrics"
		event_type: "metrics"
	},
	{
		sink:       "aws_ecs_metrics"
		service:    "aws_ecs"
		event_type: "metrics"
	},
	{
		source:     "aws_ecs_metrics"
		service:    "aws_ecs"
		event_type: "metrics"
	},
	{
		sink:       "aws_kinesis_firehose"
		service:    "aws_kinesis_firehose"
		event_type: "logs"
	},
	{
		source:     "aws_kinesis_firehose"
		service:    "aws_kinesis_firehose"
		event_type: "logs"
	},
	{
		sink:       "aws_kinesis_streams"
		service:    "aws_kinesis_data_streams"
		event_type: "logs"
	},
	{
		sink:       "aws_s3"
		service:    "aws_s3"
		event_type: "logs"
	},
	{
		source:     "aws_s3"
		service:    "aws_s3"
		event_type: "logs"
	},
	{
		sink:       "azure_monitor_logs"
		service:    "azure_monitor_logs"
		event_type: "logs"
	},
	{
		sink:       "clickhouse"
		service:    "clickhouse"
		event_type: "logs"
	},
	{
		sink:       "datadog_logs"
		service:    "datadog_logs"
		event_type: "logs"
	},
	{
		source:     "datadog_logs"
		service:    "datadog_logs"
		event_type: "logs"
	},
	{
		sink:       "datadog_metrics"
		service:    "datadog_metrics"
		event_type: "metrics"
	},
	{
		platform:   "docker"
		service:    "docker"
		event_type: "logs"
	},
	{
		sink:       "elasticsearch"
		service:    "elasticsearch"
		event_type: "logs"
	},
	{
		source:     "exec"
		service:    "exec"
		event_type: "logs"
	},
	{
		sink:       "file"
		service:    "files"
		event_type: "logs"
	},
	{
		source:     "file"
		service:    "files"
		event_type: "logs"
	},
	{
		sink:       "gcp_stackdriver_logs"
		service:    "gcp_cloud_monitoring"
		event_type: "logs"
	},
	{
		sink:       "gcp_stackdriver_metrics"
		service:    "gcp_cloud_monitoring"
		event_type: "metrics"
	},
	{
		sink:       "gcp_cloud_storage"
		service:    "gcp_cloud_storage"
		event_type: "logs"
	},
	{
		sink:       "gcp_pubsub"
		service:    "gcp_pubsub"
		event_type: "logs"
	},
	{
		source:     "heroku_logs"
		service:    "heroku"
		event_type: "logs"
	},
	{
		sink:       "honeycomb"
		service:    "honeycomb"
		event_type: "logs"
	},
	{
		source:     "host_metrics"
		service:    "host"
		event_type: "metrics"
	},
	{
		sink:       "http"
		service:    "http"
		event_type: "logs"
	},
	{
		source:     "http"
		service:    "http"
		event_type: "logs"
	},
	{
		sink:       "humio_logs"
		service:    "humio"
		event_type: "logs"
	},
	{
		sink:       "humio_metrics"
		service:    "humio"
		event_type: "metrics"
	},
	{
		sink:       "influxdb_logs"
		service:    "influxdb"
		event_type: "logs"
	},
	{
		sink:       "influxdb_metrics"
		service:    "influxdb"
		event_type: "metrics"
	},
	{
		source:     "journald"
		service:    "journald"
		event_type: "logs"
	},
	{
		sink:       "kafka"
		service:    "kafka"
		event_type: "logs and metrics"
	},
	{
		source:     "kafka"
		service:    "kafka"
		event_type: "logs"
	},
	{
		platform:   "kubernetes"
		service:    "kubernetes"
		event_type: "logs"
	},
	{
		sink:       "logdna"
		service:    "logdna"
		event_type: "logs"
	},
	{
		sink:       "loki"
		service:    "loki"
		event_type: "logs"
	},
	{
		source:     "mongodb_metrics"
		service:    "mongodb"
		event_type: "metrics"
	},
	{
		sink:       "nats"
		service:    "nats"
		event_type: "logs"
	},
	{
		sink:       "new_relic_logs"
		service:    "new_relic_logs"
		event_type: "logs"
	},
	{
		source:     "nginx_metrics"
		service:    "nginx"
		event_type: "metrics"
	},
	{
		sink:       "papertrail"
		service:    "papertrail"
		event_type: "logs"
	},
	{
		source:     "postgresql_metrics"
		service:    "postgresql"
		event_type: "metrics"
	},
	{
		source:     "prometheus_scrape"
		service:    "prometheus"
		event_type: "metrics"
	},
	{
		sink:       "sematext_logs"
		service:    "sematext"
		event_type: "logs"
	},
	{
		sink:       "sematext_metrics"
		service:    "sematext"
		event_type: "metrics"
	},
	{
		sink:       "socket"
		service:    "socket_receiver"
		event_type: "logs"
	},
	{
		source:     "socket"
		service:    "socket_client"
		event_type: "logs"
	},
	{
		sink:       "splunk_hec"
		service:    "splunk"
		event_type: "logs"
	},
	{
		source:     "splunk_hec"
		service:    "splunk"
		event_type: "logs"
	},
	{
		sink:       "statsd"
		service:    "statsd"
		event_type: "metrics"
	},
	{
		source:     "statsd"
		service:    "statsd"
		event_type: "metrics"
	},
	{
		source:     "stdin"
		service:    "stdin"
		event_type: "logs"
	},
	{
		source:     "syslog"
		service:    "syslog"
		event_type: "logs"
	},
]
