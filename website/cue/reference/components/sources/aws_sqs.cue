package metadata

components: sources: aws_sqs: components._aws & {
	title: "AWS SQS"

	features: {
		acknowledgements: true
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
		development:   "beta"
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
		requirements: [
			"""
				The AWS SQS source requires an SQS queue.
				""",
		]
		warnings: []
		notices: []
	}

	installation: {
		platform_name: null
	}

	configuration: {
		acknowledgements: configuration._source_acknowledgements
		poll_secs: {
			common:      true
			description: "How long to wait when polling SQS for new messages. 0-20 seconds"
			required:    false
			warnings: []
			type: uint: {
				default: 15
				unit:    "seconds"
			}
		}
		visibility_timeout_secs: {
			common:      false
			description: "The visibility timeout to use for messages in seconds. This controls how long a message is left unavailable when a Vector receives it. If a `vector` does not delete the message before the timeout expires, it will be made reavailable for another consumer; this can happen if, for example, the `vector` process crashes."
			required:    false
			warnings: ["Should be set higher than the length of time it takes to process an individual message to avoid that message being reprocessed."]
			type: uint: {
				default: 300
				unit:    "seconds"
			}
		}
		delete_message: {
			common:      true
			description: "Whether to delete the message once Vector processes it. It can be useful to set this to `false` to debug or during initial Vector setup."
			required:    false
			type: bool: default: true
		}
		client_concurrency: {
			common:      true
			description: "How many clients are receiving / acking SQS messages. Increasing may allow higher throughput. Note: the default is 1 / CPU core"
			required:    false
			warnings: []
			type: uint: {
				default: 1
				unit:    "concurrency"
			}
		}
		queue_url: {
			description: "The URL of the SQS queue to receive events from."
			required:    true
			warnings: []
			type: string: {
				examples: ["https://sqs.us-east-2.amazonaws.com/123456789012/MyQueue"]
				syntax: "literal"
			}
		}
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

	telemetry: metrics: {
		component_received_event_bytes_total: components.sources.internal_metrics.output.metrics.component_received_event_bytes_total
		component_received_events_total:      components.sources.internal_metrics.output.metrics.component_received_events_total
		component_received_bytes_total:       components.sources.internal_metrics.output.metrics.component_received_bytes_total
		sqs_message_delete_failed_total:      components.sources.internal_metrics.output.metrics.sqs_message_delete_failed_total
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
