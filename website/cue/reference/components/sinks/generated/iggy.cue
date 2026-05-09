package metadata

generated: components: sinks: iggy: configuration: {
	acknowledgements: {
		description: """
			Controls how acknowledgements are handled for this sink.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[e2e_acks]: https://vector.dev/docs/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type:     _schemaDefinitions["bool_or_struct::vector_core::config::AcknowledgementsConfig"]
	}
	batch: {
		description: "Event batching behavior."
		required:    false
		type: object: options: {
			max_bytes: {
				description: """
					The maximum size of a batch that is processed by a sink.

					This is based on the uncompressed size of the batched events, before they are
					serialized or compressed.
					"""
				required: false
				type: uint: {
					default: 1000000
					unit:    "bytes"
				}
			}
			max_events: {
				description: "The maximum size of a batch before it is flushed."
				required:    false
				type: uint: {
					default: 1000
					unit:    "events"
				}
			}
			timeout_secs: {
				description: "The maximum age of a batch before it is flushed."
				required:    false
				type: float: {
					default: 1.0
					unit:    "seconds"
				}
			}
		}
	}
	encoding: {
		description: """
			Encoding configuration.
			Configures how events are encoded into raw bytes.
			The selected encoding also determines which input types (logs, metrics, traces) are supported.
			"""
		required: true
		type:     _schemaDefinitions["codecs::encoding::config::EncodingConfig"]
	}
	partitions: {
		description: """
			Number of partitions to create when the topic is created on connect. Has
			no effect if the topic already exists.
			"""
		required: false
		type: uint: {
			default: 1
			examples: [
				1
			]
		}
	}
	request: {
		description: """
			Middleware settings for outbound requests.

			Various settings can be configured, such as concurrency and rate limits, timeouts, and retry behavior.

			Note that the retry backoff policy follows the Fibonacci sequence.
			"""
		required: false
		type:     _schemaDefinitions["derived::vector::sinks::util::service::TowerRequestConfig::bb2440a04988b7e322be398c"]
	}
	stream: {
		description: """
			The Iggy stream name to publish messages to. Created on connect if it does
			not already exist.
			"""
		required: true
		type: string: examples: [
			"vector"
		]
	}
	topic: {
		description: """
			The Iggy topic name within the stream. Created on connect if it does not
			already exist.
			"""
		required: true
		type: string: examples: [
			"logs"
		]
	}
	url: {
		description: """
			The Iggy [connection string][iggy_conn] of the server to publish to.

			The connection string takes the form
			`iggy+<protocol>://<credentials>@<host>:<port>` where `<protocol>` is one
			of `tcp`, `quic`, `http`, or `ws`, and `<credentials>` is either
			`username:password` or a personal access token. The legacy
			`iggy://user:pass@host:port` form (TCP, no explicit protocol) is also
			accepted.

			[iggy_conn]: https://iggy.apache.org/docs/connection-string
			"""
		required: true
		type: string: examples: ["iggy+tcp://iggy:iggy@127.0.0.1:8090", "iggy+tcp://iggypat-1234567890abcdef@host:8090"]
	}
}
