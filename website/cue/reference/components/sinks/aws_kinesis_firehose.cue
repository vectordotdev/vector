package metadata

components: sinks: aws_kinesis_firehose: components._aws & {
	title: "AWS Kinesis Firehose"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		development:   "stable"
		egress_method: "batch"
		service_providers: ["AWS"]
		stateful: false
	}

	features: {
		buffer: enabled:      true
		healthcheck: enabled: true
		send: {
			batch: {
				enabled:      true
				common:       false
				max_bytes:    4000000
				max_events:   500
				timeout_secs: 1
			}
			compression: {
				enabled: true
				default: "none"
				algorithms: ["none", "gzip"]
				levels: ["none", "fast", "default", "best", 0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
			}
			encoding: {
				enabled: true
				codec: {
					enabled: true
					enum: ["json", "text", "ndjson"]
				}
			}
			proxy: enabled: true
			request: {
				enabled: true
				headers: false
			}
			tls: {
				enabled: true
            	can_enable: false
            	can_verify_certificate: true
            	can_verify_hostname: true
            	enabled_default: false
            }
            to: {
				service: services.aws_kinesis_firehose

				interface: {
					socket: {
						api: {
							title: "AWS Kinesis Firehose API"
							url:   urls.aws_kinesis_firehose_api
						}
						direction: "outgoing"
						protocols: ["http"]
						ssl: "required"
					}
				}
			}
		}
	}

	support: {
		requirements: []
		notices: []
		warnings: []
	}

	configuration: {
		stream_name: {
			description: "The [stream name](\(urls.aws_cloudwatch_logs_stream_name)) of the target Kinesis Firehose delivery stream."
			required:    true
			type: string: {
				examples: ["my-stream"]
			}
		}
	}

	input: {
		logs:    true
		metrics: null
	}

	permissions: iam: [
		{
			platform: "aws"
			_service: "firehose"

			policies: [
				{
					_action: "DescribeDeliveryStream"
					required_for: ["healthcheck"]
				},
				{
					_action: "PutRecordBatch"
				},
			]
		},
	]

	telemetry: metrics: {
		component_sent_events_total:      components.sources.internal_metrics.output.metrics.component_sent_events_total
		component_sent_event_bytes_total: components.sources.internal_metrics.output.metrics.component_sent_event_bytes_total
	}
}
