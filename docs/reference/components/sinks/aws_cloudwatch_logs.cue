package metadata

components: sinks: aws_cloudwatch_logs: {
	title:       "AWS Cloudwatch Logs"
	description: "[Amazon CloudWatch][urls.aws_cloudwatch] is a monitoring and management service that provides data and actionable insights for AWS, hybrid, and on-premises applications and infrastructure resources. With CloudWatch, you can collect and access all your performance and operational data in form of logs and metrics from a single platform."

	classes: {
		commonly_used: true
		delivery:      "at_least_once"
		development:   "stable"
		egress_method: "batch"
		service_providers: ["AWS"]
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
					default: null
					enum: ["json", "text"]
				}
			}
			request: {
				enabled:                    true
				auto_concurrency:           false
				in_flight_limit:            5
				rate_limit_duration_secs:   1
				rate_limit_num:             5
				retry_initial_backoff_secs: 1
				retry_max_duration_secs:    10
				timeout_secs:               30
			}
			tls: enabled: false
			to: {
				name:     "AWS Cloudwatch logs"
				thing:    "an \(name) stream"
				url:      urls.aws_cloudwatch_logs
				versions: null

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
		platforms: {
			"aarch64-unknown-linux-gnu":  true
			"aarch64-unknown-linux-musl": true
			"x86_64-apple-darwin":        true
			"x86_64-pc-windows-msv":      true
			"x86_64-unknown-linux-gnu":   true
			"x86_64-unknown-linux-musl":  true
		}

		requirements: []
		warnings: []
		notices: []
	}

	configuration: {
		create_missing_group: {
			common:      true
			description: "Dynamically create a [log group][urls.aws_cloudwatch_logs_group_name] if it does not already exist. This will ignore `create_missing_stream` directly after creating the group and will create the first stream."
			required:    false
			type: bool: default: true
		}
		create_missing_stream: {
			common:      true
			description: "Dynamically create a [log stream][urls.aws_cloudwatch_logs_stream_name] if it does not already exist."
			required:    false
			type: bool: default: true
		}
		group_name: {
			description: "The [group name][urls.aws_cloudwatch_logs_group_name] of the target CloudWatch Logs stream."
			required:    true
			type: string: {
				examples: ["group-name", "{{ file }}"]
				templateable: true
			}
		}
		stream_name: {
			description: "The [stream name][urls.aws_cloudwatch_logs_stream_name] of the target CloudWatch Logs stream."
			required:    true
			type: string: {
				examples: ["{{ host }}", "%Y-%m-%d", "stream-name"]
				templateable: true
			}
		}
	}

	input: {
		logs:    true
		metrics: null
	}

	telemetry: metrics: {
		processing_errors_total: _vector_processing_errors_total
	}
}
