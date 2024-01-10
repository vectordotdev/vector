package metadata

components: sinks: aws_sqs: components._aws & {
	title: "Amazon Simple Queue Service (SQS)"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		development:   "stable"
		egress_method: "stream"
		service_providers: ["AWS"]
		stateful: false
	}

	features: {
		acknowledgements: true
		auto_generated:   true
		healthcheck: enabled: true
		send: {
			compression: enabled: false
			encoding: {
				enabled: true
				codec: {
					enabled: true
					enum: ["json", "text"]
				}
			}
			proxy: enabled: true
			request: {
				enabled:                    true
				rate_limit_duration_secs:   1
				rate_limit_num:             5
				retry_initial_backoff_secs: 1
				retry_max_duration_secs:    10
				timeout_secs:               30
				headers:                    false
			}
			tls: {
				enabled:                true
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        true
				enabled_by_scheme:      true
			}
			to: {
				service: services.aws_sqs

				interface: {
					socket: {
						api: {
							title: "Amazon Simple Queue Service API"
							url:   urls.aws_sqs_api
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
		warnings: []
		notices: []
	}

	configuration: base.components.sinks.aws_sqs.configuration & {
		_aws_include: false
	}

	input: {
		logs:    true
		metrics: null
		traces:  false
	}

	permissions: iam: [
		{
			platform:  "aws"
			_service:  "sqs"
			_docs_tag: "AWSSimpleQueueService"

			policies: [
				{
					_action: "GetQueueAttributes"
					required_for: ["healthcheck"]
				},
				{
					_action: "SendMessage"
				},
			]
		},
	]
}
