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
		acknowledgements: true
		auto_generated:   true
		healthcheck: enabled: true
		send: {
			batch: {
				enabled:      true
				common:       false
				max_bytes:    1048576
				max_events:   10000
				timeout_secs: 1.0
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
					enum: ["json", "text"]
				}
			}
			proxy: enabled: true
			request: {
				enabled: true
				headers: true
			}
			tls: {
				enabled:                true
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        true
				enabled_by_scheme:      true
			}
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

	configuration: base.components.sinks.aws_cloudwatch_logs.configuration & {
		_aws_include: false
	}

	input: {
		logs:    true
		metrics: null
		traces:  false
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
}
