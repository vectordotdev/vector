package metadata

components: sources: gcp_pubsub: {
	title: "GCP Pub/Sub"

	features: {
		auto_generated:   true
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

	configuration: base.components.sources.gcp_pubsub.configuration

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
