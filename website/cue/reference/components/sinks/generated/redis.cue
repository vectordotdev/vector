package metadata

generated: components: sinks: redis: configuration: {
	acknowledgements: {
		description: """
			Controls how acknowledgements are handled for this sink.

			See [End-to-end Acknowledgements][e2e_acks] for more information on how event acknowledgement is handled.

			[e2e_acks]: https://vector.dev/docs/architecture/end-to-end-acknowledgements/
			"""
		required: false
		type:     _schemaDefinitions["vector_core::config::AcknowledgementsConfig"]
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
					default: 1
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
	dangerously_allow_unconfined_template_resolution: {
		description: """
			Disable all template confinement checks for this sink.

			**DANGEROUS — disables a security control.**

			Bypasses both startup validation and runtime confinement for every
			templated field on this sink. When enabled, a log producer that
			controls any field used in a template can write to arbitrary keys,
			paths, or routing destinations. This flag is a full opt-out: it
			disables confinement even for templates that have a usable static
			prefix.
			"""
		required: false
		type: bool: default: false
	}
	data_type: {
		description: "Redis data type to store messages in."
		required:    false
		type: string: {
			default: "list"
			enum: {
				channel: """
					The Redis `channel` type.

					Redis channels function in a pub/sub fashion, allowing many-to-many broadcasting and receiving.
					"""
				list: """
					The Redis `list` type.

					This resembles a deque, where messages can be popped and pushed from either end.

					This is the default.
					"""
				sortedset: """
					The Redis `sorted set` type.

					This resembles a priority queue, where messages can be pushed and popped with an
					associated score.
					"""
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
	endpoint: {
		description: """
			The URL of the Redis endpoint to connect to.

			The URL _must_ take the form of `protocol://server:port/db` where the protocol can either be
			`redis` or `rediss` for connections secured via TLS.
			"""
		required: true
		type: string: examples: ["redis://127.0.0.1:6379/0"]
	}
	key: {
		description: "The Redis key to publish messages to."
		required:    true
		type: string: {
			examples: ["syslog:{{ app }}", "vector"]
			syntax: "template"
		}
	}
	list_option: {
		description: "List-specific options."
		required:    false
		type: object: options: method: {
			description: "The method to use for pushing messages into a `list`."
			required:    true
			type: string: enum: {
				lpush: """
					Use the `lpush` method.

					This pushes messages onto the head of the list.
					"""
				rpush: """
					Use the `rpush` method.

					This pushes messages onto the tail of the list.

					This is the default.
					"""
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
		type: object: options: {
			adaptive_concurrency: {
				description: """
					Configuration of adaptive concurrency parameters.

					These parameters typically do not require changes from the default, and incorrect values can lead to meta-stable or
					unstable performance and sink behavior. Proceed with caution.
					"""
				required: false
				type:     _schemaDefinitions["vector::sinks::util::adaptive_concurrency::AdaptiveConcurrencySettings"]
			}
			concurrency: {
				description: """
					Configuration for outbound request concurrency.

					This can be set either to one of the below enum values or to a positive integer, which denotes
					a fixed concurrency limit.
					"""
				required: false
				type: {
					string: {
						default: "none"
						enum: {
							adaptive: """
															Concurrency is managed by Vector's [Adaptive Request Concurrency][arc] feature.

															[arc]: https://vector.dev/docs/architecture/arc/
															"""
							none: """
															A fixed concurrency of 1.

															Only one request can be outstanding at any given time.
															"""
						}
					}
					uint: {}
				}
			}
			rate_limit_duration_secs: {
				description: "The time window used for the `rate_limit_num` option."
				required:    false
				type: uint: {
					default: 1
					unit:    "seconds"
				}
			}
			rate_limit_num: {
				description: "The maximum number of requests allowed within the `rate_limit_duration_secs` time window."
				required:    false
				type: uint: {
					default: 9223372036854775807
					unit:    "requests"
				}
			}
			retry_attempts: {
				description: "The maximum number of retries to make for failed requests."
				required:    false
				type: uint: {
					default: 9223372036854775807
					unit:    "retries"
				}
			}
			retry_initial_backoff_secs: {
				description: """
					The amount of time to wait before attempting the first retry for a failed request.

					After the first retry has failed, the Fibonacci sequence is used to select future backoffs.
					"""
				required: false
				type: uint: {
					default: 1
					unit:    "seconds"
				}
			}
			retry_jitter_mode: {
				description: "The jitter mode to use for retry backoff behavior."
				required:    false
				type: string: {
					default: "Full"
					enum: {
						Full: """
															Full jitter.

															The random delay is anywhere from 0 up to the maximum current delay calculated by the backoff
															strategy.

															Incorporating full jitter into your backoff strategy can greatly reduce the likelihood
															of creating accidental denial of service (DoS) conditions against your own systems when
															many clients are recovering from a failure state.
															"""
						None: "No jitter."
					}
				}
			}
			retry_max_duration_secs: {
				description: "The maximum amount of time to wait between retries."
				required:    false
				type: uint: {
					default: 30
					unit:    "seconds"
				}
			}
			timeout_secs: {
				description: """
					The time a request can take before being aborted.

					Datadog highly recommends that you do not lower this value below the service's internal timeout, as this could
					create orphaned requests, pile on retries, and result in duplicate data downstream.
					"""
				required: false
				type: uint: {
					default: 60
					unit:    "seconds"
				}
			}
		}
	}
	sentinel_connect: {
		description: "Controls how Redis Sentinel will connect to the servers belonging to it."
		required:    false
		type: object: options: {
			connections: {
				description: """
					Connection independent information used to establish a connection
					to a redis instance sentinel owns.
					"""
				required: false
				type: object: options: {
					db: {
						description: "The database number to use. Usually `0`."
						required:    true
						type: int: {}
					}
					password: {
						description: "Optionally, the password to connection with."
						required:    false
						type: string: {}
					}
					protocol: {
						description: "The version of RESP to use."
						required:    true
						type: string: enum: {
							RESP2: """
																			Use RESP2.

																			This is the default.
																			"""
							RESP3: "Use RESP3."
						}
					}
					username: {
						description: "Optionally, the username to connection with."
						required:    false
						type: string: {}
					}
				}
			}
			tls: {
				description: "How/if TLS should be established."
				required:    false
				type: string: {
					default: "none"
					enum: {
						insecure: "Enable TLS without certificate verification."
						none: """
															Don't use TLS.

															This is the default.
															"""
						secure: "Enable TLS with certificate verification."
					}
				}
			}
		}
	}
	sentinel_service: {
		description: """
			The service name to use for sentinel.

			If this is specified, `endpoint` will be used to reach sentinel instances instead of a
			redis instance.
			"""
		required: false
		type: string: {}
	}
	sorted_set_option: {
		description: "Sorted Set-specific options"
		required:    false
		type: object: options: {
			method: {
				description: "The method to use for pushing messages into a `sorted set`."
				required:    false
				type: string: enum: zadd: """
					Use the `zadd` method.

					This adds messages onto a queue with a score.

					This is the default.
					"""
			}
			score: {
				description: """
					The score to publish a message with to a `sorted set`.

					Examples:
					- `%s`
					- `%Y%m%d%H%M%S`
					"""
				required: false
				type: {
					string: syntax: "template"
					uint: {}
				}
			}
		}
	}
}
