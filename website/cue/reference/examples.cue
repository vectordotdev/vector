package metadata

#ConfigExample: {
	title:   !=""
	example: !=""
}

config_examples: [#ConfigExample, ...#ConfigExample] & [
	{
		title: "Redacted Datadog Agent logs to Datadog"
		example: #"""
			sources:
				datadog_agent:
					type: "datadog_agent"
					address: "0.0.0.0:80"

			transforms:
				remove_sensitive_user_info:
					type: "remap"
					inputs: ["datadog_agent"]
					source: |
						redact(., filters: ["us_social_security_number"])

			sinks:
				datadog_backend:
					type: "datadog_logs"
					inputs: ["remove_sensitive_user_info"]
					default_api_key: "${DATADOG_API_KEY}"
			"""#
	},
	{
		title: "Kafka topic to Elasticsearch"
		example: #"""
			sources:
				kafka_in:
					type: "kafka"
					bootstrap_servers: "10.14.22.123:9092,10.14.23.332:9092"
					group_id: "vector-logs"
					key_field: "message"
					topics: ["logs-*"]

			transforms:
				json_parse:
					type: "remap"
					inputs: ["kafka_in"]
					source: |
						parsed, err = parse_json(.message)
						if err != null {
							log(err, level: "error")
						}
						. |= object(parsed) ?? {}

			sinks:
				elasticsearch_out:
					type: "elasticsearch"
					inputs: ["json_parse"]
					endpoint: "http://10.24.32.122:9000"
					index: "logs-via-kafka"
			"""#
	},
	{
		title: "Kubernetes logs to AWS S3"
		example: #"""
			sources:
				k8s_in:
					type: "kubernetes_logs"

			sinks:
				aws_s3_out:
					type: "aws_s3"
					inputs: ["k8s_in"]
					bucket: "k8s-logs"
					region: "us-east-1"
					compression: "gzip"
					encoding:
						codec: "json"
			"""#
	},
	{
		title: "Splunk HEC to Datadog"
		example: #"""
			sources:
				splunk_hec_in:
					type: "splunk_hec"
					address: "0.0.0.0:8080"
					token: "${SPLUNK_HEC_TOKEN}"

			sinks:
				datadog_out:
					type: "datadog_logs"
					inputs: ["splunk_hec_in"]
					default_api_key: "${DATADOG_API_KEY}"
			"""#
	},
]
