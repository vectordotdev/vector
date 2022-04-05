package metadata

components: sources: gcp_pubsub: {
	title: "GCP Pub/Sub"

	features: {
		acknowledgements: true
		collect: {
			tls: {
				enabled:                true
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        true
			}
			checkpoint: enabled: false
			proxy: enabled:      true
			from: service:       services.gcp_pubsub
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
					The GCP Pub/Sub source requires a Pub/Sub subscription.
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
		ack_deadline_seconds: {
			common:      false
			description: "The acknowledgement deadline to use for this stream. Messages that are not acknowledged when this deadline expires may be retransmitted."
			required:    false
			type: uint: {
				default: 600
				examples: [10, 600]
				unit: "seconds"
			}
		}
		api_key:          configuration._gcp_api_key
		credentials_path: configuration._gcp_credentials_path
		endpoint: {
			common:      false
			description: "The endpoint from which to pull data."
			required:    false
			type: string: {
				default: "https://pubsub.googleapis.com"
				examples: ["https://us-central1-pubsub.googleapis.com"]
			}
		}
		project: {
			description: "The project name from which to pull logs."
			required:    true
			type: string: {
				examples: ["vector-123456"]
			}
		}
		subscription: {
			description: "The subscription within the project which is configured to receive logs."
			required:    true
			type: string: {
				examples: ["vector-123456"]
			}
		}
	}

	output: logs: record: {
		description: "An individual Pub/Sub record"
		fields: {
			attributes: {
				description: "Attributes that were published with the Pub/Sub record."
				required:    true
				type: object: {
					examples: [{"key": "value"}]
				}
			}
			message: {
				description: "The message from the Pub/Sub record, parsed from the raw data."
				required:    true
				type: string: {
					examples: ["53.126.150.246 - - [01/Oct/2020:11:25:58 -0400] \"GET /disintermediate HTTP/2.0\" 401 20308"]
					syntax: "literal"
				}
			}
			message_id: {
				description: "The ID of this message, assigned by the server when the message is published. Guaranteed to be unique within the topic."
				required:    true
				type: string: {
					examples: ["2345"]
					syntax: "literal"
				}
			}
			timestamp: fields._current_timestamp & {
				description: "The time this message was published in the topic."
			}
		}
	}

	telemetry: metrics: {
		component_received_event_bytes_total: components.sources.internal_metrics.output.metrics.component_received_event_bytes_total
		component_received_events_total:      components.sources.internal_metrics.output.metrics.component_received_events_total
		component_received_bytes_total:       components.sources.internal_metrics.output.metrics.component_received_bytes_total
	}

	how_it_works: {
		gcp_pubsub: {
			title: "GCP Pub/Sub"
			body: """
				The `gcp_pubsub` source streams messages from [GCP Pub/Sub](https://cloud.google.com/pubsub).
				This is a highly scalable / durable queueing system with at-least-once queuing semantics.
				Messages are received in a stream and are either acknowledged immediately after receiving
				or after it has been fully processed by the sink(s), depending on if any of the sink(s)
				have the `acknowledgements` setting enabled.
				"""
		}
	}
}
