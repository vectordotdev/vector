package metadata

components: sources: aws_sqs: components._aws & {
	title: "AWS SQS"

	features: {
		acknowledgements: true
		auto_generated:   true
		collect: {
			tls: {
				enabled:                true
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        true
				enabled_by_scheme:      true
			}
			checkpoint: enabled: false
			proxy: enabled:      true
			from: service:       services.aws_sqs
		}
		multiline: enabled: false
		codecs: {
			enabled:         true
			default_framing: "bytes"
		}
	}

	classes: {
		commonly_used: true
		deployment_roles: ["aggregator"]
		delivery:      "at_least_once"
		development:   "stable"
		egress_method: "stream"
		stateful:      false
	}

	support: {
		targets: {
			"aarch64-unknown-linux-gnu":      true
			"aarch64-unknown-linux-musl":     true
			"armv7-unknown-linux-gnueabihf":  true
			"armv7-unknown-linux-musleabihf": true
			"x86_64-apple-darwin":            true
			"x86_64-pc-windows-msv":          true
			"x86_64-unknown-linux-gnu":       true
			"x86_64-unknown-linux-musl":      true
		}
		requirements: []
		warnings: []
		notices: []
	}

	installation: {
		platform_name: null
	}

	configuration: base.components.sources.aws_sqs.configuration & {
		_aws_include: false
	}

	output: logs: record: {
		description: "An individual SQS record"
		fields: {
			message: {
				description: "The raw message from the SQS record."
				required:    true
				type: string: {
					examples: ["53.126.150.246 - - [01/Oct/2020:11:25:58 -0400] \"GET /disintermediate HTTP/2.0\" 401 20308"]
					syntax: "literal"
				}
			}
			source_type: {
				description: "The name of the source type."
				required:    true
				type: string: {
					examples: ["aws_sqs"]
				}
			}
			timestamp: fields._current_timestamp & {
				description: "The time this message was sent to SQS."
			}
		}
	}

	how_it_works: {
		aws_sqs: {
			title: "AWS SQS"
			body: """
				The `aws_sqs` source receives messages from [AWS SQS](https://aws.amazon.com/sqs/)
				(Simple Queue Service). This is a highly scalable / durable queueing system with
				at-least-once queuing semantics. Messages are received in batches (up to 10 at a time),
				and then deleted in batches (again up to 10). Messages are either deleted immediately
				after receiving, or after it has been fully processed by the sinks, depending on the
				`acknowledgements` setting.
				"""
		}
	}
}
