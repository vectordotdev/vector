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
			description: "The acknowledgement deadline to use for this stream. Messages that are not acknowledged when this deadline expires may be retransmitted. This setting is deprecated and will be removed in a future version."
			required:    false
			type: uint: {
				default: 600
				examples: [10, 600]
				unit: "seconds"
			}
		}
		ack_deadline_secs: {
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
		full_response_size: {
			common: false
			description: """
					The number of messages in a response to mark a stream as "busy".
					This is used to determine if more streams should be started.
					The GCP Pub/Sub servers send responses with 100 or more messages when
					the subscription is busy.
				"""
			required: false
			type: uint: {
				default: 100
				examples: [100, 128]
				unit: null
			}
		}
		keepalive_secs: {
			common:      false
			description: "The amount of time, in seconds, with no received activity before sending a keepalive request. If this is set larger than `60`, you may see periodic errors sent from the server."
			required:    false
			type: float: {
				default: 60.0
				examples: [10.0]
			}
		}
		max_concurrency: {
			common:      false
			description: "The maximum number of concurrent stream connections to open at once."
			required:    false
			type: uint: {
				default: 5
				examples: [1, 9]
				unit: "concurrency"
			}
		}
		poll_time_seconds: {
			common:      false
			description: "How often to poll the currently active streams to see if they are all busy and so open a new stream."
			required:    false
			type: float: {
				default: 2.0
				examples: [1.0, 5.0]
				unit: "seconds"
			}
		}
		project: {
			description: "The project name from which to pull logs."
			required:    true
			type: string: {
				examples: ["vector-123456"]
			}
		}
		retry_delay_seconds: {
			common:      false
			description: "The amount of time to wait between retry attempts after an error. This setting is deprecated and will be removed in a future version."
			required:    false
			type: float: {
				default: 1.0
				examples: [0.5]
				unit: "seconds"
			}
		}
		retry_delay_secs: {
			common:      false
			description: "The amount of time to wait between retry attempts after an error."
			required:    false
			type: float: {
				default: 1.0
				examples: [0.5]
				unit: "seconds"
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
			source_type: {
				description: "The name of the source type."
				required:    true
				type: string: {
					examples: ["gcp_pubsub"]
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
		auto_concurrency: {
			title: "Automatic Concurrency Management"
			body: """
					The `gcp_pubsub` source automatically manages the number of concurrent active streams by
					monitoring the traffic flowing over the streams.
					When a stream receives full responses (as determined by the `full_response_size` setting),
					it marks itself as being "busy".
					Periodically, the source will poll all the active connections and will start a new stream
					if all the active streams are marked as busy and fewer than `max_concurrency` streams are
					active.
					Conversely, when a stream passes an idle interval (configured by the
					`idle_timeout_seconds` setting) with no traffic and no outstanding acknowledgements,
					it will drop the connection unless there are no other streams active.
					This combination of actions allows this source to respond dynamically to high load levels
					without opening up extra connections at startup.
				"""
		}
	}
}
