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
		acknowledgements: true
		auto_generated:   true
		healthcheck: enabled: true
		send: {
			batch: {
				enabled:      true
				common:       false
				max_bytes:    4000000
				max_events:   500
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
				headers: false
			}
			tls: {
				enabled:                true
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        true
				enabled_by_scheme:      true
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

	configuration: base.components.sinks.aws_kinesis_firehose.configuration & {
		_aws_include: false
		request_retry_partial: warnings: ["This can cause duplicate logs to be published."]
	}

	input: {
		logs:    true
		metrics: null
		traces:  false
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
}
