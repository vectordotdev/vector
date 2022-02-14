package metadata

components: sinks: aws_cloudwatch_logs: components._aws & {
	title: "AWS Cloudwatch Logs"

	classes: {
		commonly_used: true
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
				max_bytes:    1048576
				max_events:   10000
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
			tls: enabled: false
			to: {
				service: services.aws_cloudwatch_logs

				interface: {
					socket: {
						api: {
							title: "AWS Cloudwatch logs API"
							url:   urls.aws_cloudwatch_logs_api
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
		create_missing_group: {
			common:      true
			description: "Dynamically create a [log group](\(urls.aws_cloudwatch_logs_group_name)) if it does not already exist. This will ignore `create_missing_stream` directly after creating the group and will create the first stream."
			required:    false
			type: bool: default: true
		}
		create_missing_stream: {
			common:      true
			description: "Dynamically create a [log stream](\(urls.aws_cloudwatch_logs_stream_name)) if it does not already exist."
			required:    false
			type: bool: default: true
		}
		group_name: {
			description: "The [group name](\(urls.aws_cloudwatch_logs_group_name)) of the target CloudWatch Logs stream."
			required:    true
			type: string: {
				examples: ["group-name", "{{ file }}"]
				syntax: "template"
			}
		}
		stream_name: {
			description: "The [stream name](\(urls.aws_cloudwatch_logs_stream_name)) of the target CloudWatch Logs stream. Note that there can only be one writer to a log stream at a time so if you are running multiple vectors all writing to the same log group, include a identifier in the stream name that is guaranteed to be unique by vector instance (for example, you might choose `host`)"
			required:    true
			type: string: {
				examples: ["{{ host }}", "%Y-%m-%d", "stream-name"]
				syntax: "template"
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
			_service: "AmazonCloudWatchLogs"

			policies: [
				{
					_action:       "CreateLogGroup"
					required_when: "[`create_missing_group`](#create_missing_group) is set to `true`"
				},
				{
					_action:       "CreateLogStream"
					required_when: "[`create_missing_stream`](#create_missing_stream) is set to `true`"
				},
				{
					_action: "DescribeLogGroups"
					required_for: ["healthcheck"]
				},
				{
					_action: "DescribeLogStreams"
				},
				{
					_action: "PutLogEvents"
				},
			]
		},
	]

	telemetry: metrics: {
		component_sent_events_total:      components.sources.internal_metrics.output.metrics.component_sent_events_total
		component_sent_event_bytes_total: components.sources.internal_metrics.output.metrics.component_sent_event_bytes_total
		events_discarded_total:           components.sources.internal_metrics.output.metrics.events_discarded_total
		processing_errors_total:          components.sources.internal_metrics.output.metrics.processing_errors_total
	}
}
