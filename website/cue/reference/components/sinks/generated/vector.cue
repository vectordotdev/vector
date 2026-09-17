package metadata

generated: components: sinks: vector: configuration: {
	acknowledgements: {
		description: """
			Controls how acknowledgements are handled for this sink.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[e2e_acks]: https://vector.dev/docs/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type:     _schemaDefinitions["vector_core::config::AcknowledgementsConfig"]
	}
	address: {
		deprecated:         true
		deprecated_message: "This option has been deprecated, use `routing.endpoints` instead."
		description: """
			The downstream Vector address to which to connect.

			Both IP address and hostname are accepted formats.

			The address _must_ include a port.

			This option is mutually exclusive with `routing`. Set exactly one of
			`address` or `routing`.

			This option has been deprecated, use `routing.endpoints` instead.

			Exactly one of `address` or `routing` must be set.
			"""
		required: false
		required_one_of: ["address", "routing"]
		required_one_of_group: "address_or_routing"
		type: string: examples: ["http://127.0.0.1:6000", "https://somehost:6000"]
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
				type: uint: unit: "bytes"
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
	compression: {
		description: """
			Compression algorithm for requests.

			Supports `"none"`, `"gzip"`, or `"zstd"`.
			"""
		required: false
		type: string: {
			default: "none"
			enum: {
				gzip: """
					[Gzip][gzip] compression.

					[gzip]: https://www.gzip.org/
					"""
				none: "No compression."
				zstd: """
					[Zstandard][zstd] compression.

					[zstd]: https://facebook.github.io/zstd/
					"""
			}
		}
	}
	keepalive: {
		description: """
			HTTP/2 keepalive settings for the sink's gRPC connections.

			Keepalive is disabled unless this is configured. When enabled, the sink sends HTTP/2 PING
			frames on idle connections so that a pooled connection to a downstream Vector instance that
			has gone away (crashed, restarted, or cut off by a network partition) is detected and evicted
			before it is reused, ensuring retries always go to a live connection.
			"""
		required: false
		type: object: options: {
			interval_secs: {
				description: """
					How often, in seconds, to send a keepalive PING on idle connections.

					Shorter intervals detect dead connections faster at the cost of additional traffic.
					gRPC guidance recommends no less than 60 seconds to avoid tripping `too_many_pings`
					policies on servers or proxies between source and destination.
					"""
				required: false
				type: uint: default: 60
			}
			timeout_secs: {
				description: """
					How long, in seconds, to wait for a keepalive PING acknowledgement before treating
					the connection as dead and closing it.
					"""
				required: false
				type: uint: default: 20
			}
		}
	}
	request: {
		description: """
			Middleware settings for outbound requests.

			Various settings can be configured, such as concurrency and rate limits, timeouts, and retry behavior.

			Note that the retry backoff policy follows the Fibonacci sequence.
			"""
		required: false
		type:     _schemaDefinitions["vector::sinks::util::service::TowerRequestConfig"]
	}
	routing: {
		description: """
			Routing options for sending requests to one or more downstream Vector endpoints.

			This option is mutually exclusive with `address`. Set exactly one of
			`address` or `routing`.

			Exactly one of `address` or `routing` must be set.
			"""
		required: false
		required_one_of: ["address", "routing"]
		required_one_of_group: "address_or_routing"
		type: object: options: {
			endpoints: {
				description: """
					The downstream Vector endpoints to which to connect.

					Both IP addresses and hostnames are accepted formats.

					Each endpoint _must_ include a port.
					"""
				required: false
				type: array: {
					default: []
					items: type: string: examples: ["https://127.0.0.1:6000", "https://somehost:6000"]
				}
			}
			health: {
				description: """
					Options for determining the health and backoff behavior of
					load-balanced Vector endpoints.

					This option is only used when `strategy` is set to `load_balance`.
					"""
				required: false
				type:     _schemaDefinitions["core::option::Option<vector::sinks::util::service::health::HealthConfig>"]
			}
			strategy: {
				description: """
					Strategy for routing requests across configured endpoints.

					When only one endpoint is configured, the sink uses the standard
					single-endpoint service path and strategy-specific routing semantics are
					not applied.
					"""
				required: false
				type: string: {
					default: "load_balance"
					enum: {
						failover: """
															Use one endpoint at a time. When the active endpoint fails, continue
															through the configured endpoints from the next endpoint.

															This mode keeps using the last successful endpoint until it fails. Use
															`failover_primary` instead when retriable failures should re-check the
															first configured endpoint before trying secondary endpoints.

															Requests are serialized for this strategy, regardless of the configured
															request concurrency, to preserve one active endpoint at a time.
															"""
						failover_primary: """
															Use one endpoint at a time. When the active endpoint fails, retry from
															the configured endpoint order so the sink can return to its configured
															primary endpoint.

															This is useful when receiver-side connection recycling, such as
															`max_connection_age_secs`, should converge the sink back to the first
															configured endpoint when it is available.

															Requests are serialized for this strategy, regardless of the configured
															request concurrency, to preserve one active endpoint at a time.
															"""
						load_balance: """
															Distribute requests across healthy endpoints using Vector's existing
															Tower distributed service. Endpoint health is tracked using
															`routing.health`, and unhealthy endpoints are backed off and probed
															according to that configuration. This mode does not preserve a single
															active endpoint or prefer the first configured endpoint.
															"""
					}
				}
			}
		}
	}
	tls: {
		description: "Configures the TLS options for incoming/outgoing connections."
		required:    false
		type:        _schemaDefinitions["core::option::Option<vector_core::tls::settings::TlsEnableableConfig>"]
	}
}
