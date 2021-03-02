package metadata

import (
	"list"
)

components: sinks: [Name=string]: {
	kind: "sink"

	features: _

	configuration: {
		if features.send != _|_ && features.send.batch != _|_ {
			if features.send.batch.enabled {
				batch: {
					common:      features.send.batch.common
					description: "Configures the sink batching behavior."
					required:    false
					type: object: {
						examples: []
						options: {
							if features.send.batch.max_bytes != null {
								max_bytes: {
									common:      true
									description: "The maximum size of a batch, in bytes, before it is flushed."
									required:    false
									type: uint: {
										default: features.send.batch.max_bytes
										unit:    "bytes"
									}
								}
							}
							if features.send.batch.max_events != null {
								max_events: {
									common:      true
									description: "The maximum size of a batch, in events, before it is flushed."
									required:    false
									type: uint: {
										default: features.send.batch.max_events
										unit:    "events"
									}
								}
							}
							if features.send.batch.timeout_secs != null {
								timeout_secs: {
									common:      true
									description: "The maximum age of a batch before it is flushed."
									required:    false
									type: uint: {
										default: features.send.batch.timeout_secs
										unit:    "seconds"
									}
								}
							}
						}
					}
				}
			}
		}

		if features.buffer.enabled {
			buffer: {
				common:      false
				description: "Configures the sink specific buffer behavior."
				required:    false
				type: object: {
					examples: []
					options: {
						max_events: {
							common:        true
							description:   "The maximum number of [events][docs.data-model] allowed in the buffer."
							required:      false
							relevant_when: "type = \"memory\""
							type: uint: {
								default: 500
								unit:    "events"
							}
						}
						max_size: {
							description:   "The maximum size of the buffer on the disk."
							required:      true
							relevant_when: "type = \"disk\""
							type: uint: {
								examples: [104900000]
								unit: "bytes"
							}
						}
						type: {
							common:      true
							description: "The buffer's type and storage mechanism."
							required:    false
							type: string: {
								default: "memory"
								enum: {
									memory: "Stores the sink's buffer in memory. This is more performant, but less durable. Data will be lost if Vector is restarted forcefully."
									disk:   "Stores the sink's buffer on disk. This is less performant, but durable. Data will not be lost between restarts."
								}
								syntax: "literal"
							}
						}
						when_full: {
							common:      false
							description: "The behavior when the buffer becomes full."
							required:    false
							type: string: {
								default: "block"
								enum: {
									block:       "Applies back pressure when the buffer is full. This prevents data loss, but will cause data to pile up on the edge."
									drop_newest: "Drops new data as it's received. This data is lost. This should be used when performance is the highest priority."
								}
								syntax: "literal"
							}
						}
					}
				}
			}
		}

		if features.send != _|_ {
			if features.send.compression.enabled {
				compression: {
					common:      true
					description: "The compression strategy used to compress the encoded event data before transmission."
					required:    false
					type: string: {
						default: features.send.compression.default
						enum: {
							if list.Contains(features.send.compression.algorithms, "none") {
								none:   "No compression."
								syntax: "literal"
							}
							if list.Contains(features.send.compression.algorithms, "gzip") {
								gzip: "[Gzip](\(urls.gzip)) standard DEFLATE compression."
							}
						}
						syntax: "literal"
					}
				}
			}
		}

		if features.send != _|_ {
			if features.send.encoding.enabled {
				encoding: {
					description: "Configures the encoding specific sink behavior."
					required:    true
					type: object: options: {
						if features.send.encoding.codec.enabled {
							codec: {
								description: "The encoding codec used to serialize the events before outputting."
								required:    true
								type: string: {
									examples: features.send.encoding.codec.enum
									syntax:   "literal"
								}
							}
						}

						if features.healthcheck.enabled {except_fields: {
							common:      false
							description: "Prevent the sink from encoding the specified labels."
							required:    false
							type: array: {
								default: null
								items: type: string: {
									examples: ["message", "parent.child"]
									syntax: "field_path"
								}
							}
						}

							only_fields: {
								common:      false
								description: "Prevent the sink from encoding the specified labels."
								required:    false
								type: array: {
									default: null
									items: type: string: {
										examples: ["message", "parent.child"]
										syntax: "field_path"
									}
								}
							}

							timestamp_format: {
								common:      false
								description: "How to format event timestamps."
								required:    false
								type: string: {
									default: "rfc3339"
									enum: {
										rfc3339: "Formats as a RFC3339 string"
										unix:    "Formats as a unix timestamp"
									}
									syntax: "literal"
								}
							}
						}
					}
				}
			}
		}

		if features.healthcheck != _|_ {
			if features.healthcheck.enabled {
				healthcheck: {
					common:      true
					description: "Health check options for the sink."
					required:    false
					type: object: {
						examples: []
						options: {
							enabled: {
								common:      true
								description: "Enables/disables the healthcheck upon Vector boot."
								required:    false
								type: bool: default: true
							}
						}
					}
				}
			}
		}

		if features.send != _|_ {
			if features.send.request.enabled {
				request: {
					common:      false
					description: "Configures the sink request behavior."
					required:    false
					type: object: {
						examples: []
						options: {
							adaptive_concurrency: {
								common:      false
								description: "Configure the adaptive concurrency algorithms. These values have been tuned by optimizing simulated results. In general you should not need to adjust these."
								required:    false
								type: object: {
									examples: []
									options: {
										decrease_ratio: {
											common:      false
											description: "The fraction of the current value to set the new concurrency limit when decreasing the limit. Valid values are greater than 0 and less than 1. Smaller values cause the algorithm to scale back rapidly when latency increases. Note that the new limit is rounded down after applying this ratio."
											required:    false
											type: float: default: 0.9
										}
										ewma_alpha: {
											common:      false
											description: "The adaptive concurrency algorithm uses an exponentially weighted moving average (EWMA) of past RTT measurements as a reference to compare with the current RTT. This value controls how heavily new measurements are weighted compared to older ones. Valid values are greater than 0 and less than 1. Smaller values cause this reference to adjust more slowly, which may be useful if a service has unusually high response variability."
											required:    false
											type: float: default: 0.7
										}
										rtt_threshold_ratio: {
											common:      false
											description: "When comparing the past RTT average to the current measurements, we ignore changes that are less than this ratio higher than the past RTT. Valid values are greater than or equal to 0. Larger values cause the algorithm to ignore larger increases in the RTT."
											required:    false
											type: float: default: 0.05
										}
									}
								}
							}
							concurrency: {
								common: true
								if features.send.request.adaptive_concurrency {
									description: "The maximum number of in-flight requests allowed at any given time, or \"adaptive\" to allow Vector to automatically set the limit based on current network and service conditions."
								}
								if !features.send.request.adaptive_concurrency {
									description: "The maximum number of in-flight requests allowed at any given time."
								}
								required: false
								type: uint: {
									default: features.send.request.concurrency
									unit:    "requests"
								}
							}
							rate_limit_duration_secs: {
								common:      true
								description: "The time window, in seconds, used for the `rate_limit_num` option."
								required:    false
								type: uint: {
									default: features.send.request.rate_limit_duration_secs
									unit:    "seconds"
								}
							}
							rate_limit_num: {
								common:      true
								description: "The maximum number of requests allowed within the `rate_limit_duration_secs` time window."
								required:    false
								type: uint: {
									default: features.send.request.rate_limit_num
									unit:    null
								}
							}
							retry_attempts: {
								common:      false
								description: "The maximum number of retries to make for failed requests. The default, for all intents and purposes, represents an infinite number of retries."
								required:    false
								type: uint: {
									default: 18446744073709552000
									unit:    null
								}
							}
							retry_initial_backoff_secs: {
								common:      false
								description: "The amount of time to wait before attempting the first retry for a failed request. Once, the first retry has failed the fibonacci sequence will be used to select future backoffs."
								required:    false
								type: uint: {
									default: features.send.request.retry_initial_backoff_secs
									unit:    "seconds"
								}
							}
							retry_max_duration_secs: {
								common:      false
								description: "The maximum amount of time, in seconds, to wait between retries."
								required:    false
								type: uint: {
									default: features.send.request.retry_max_duration_secs
									unit:    "seconds"
								}
							}
							timeout_secs: {
								common:      true
								description: "The maximum time a request can take before being aborted. It is highly recommended that you do not lower this value below the service's internal timeout, as this could create orphaned requests, pile on retries, and result in duplicate data downstream."
								required:    false
								type: uint: {
									default: features.send.request.timeout_secs
									unit:    "seconds"
								}
							}

							if features.send.request.headers {
								headers: {
									common:      false
									description: "Options for custom headers."
									required:    false
									warnings: []
									type: object: {
										examples: [
											{
												"Authorization": "${HTTP_TOKEN}"
												"X-Powered-By":  "Vector"
											},
										]
										options: {}
									}
								}
							}
						}
					}
				}
			}
		}

		if features.send != _|_ {
			if features.send.send_buffer_size != _|_ {
				send_buffer_bytes: {
					common:      false
					description: "Configures the send buffer size using the `SO_SNDBUF` option on the socket."
					required:    false
					type: uint: {
						examples: [65536]
					}
					relevant_when: features.send.send_buffer_bytes.relevant_when
				}
			}

			if features.send.keepalive != _|_ {
				keepalive: {
					common:      false
					description: "Configures the TCP keepalive behavior for the connection to the sink."
					required:    false
					type: object: {
						examples: []
						options: {
							time_secs: {
								common:      false
								description: "The time a connection needs to be idle before sending TCP keepalive probes."
								required:    false
								type: uint: {
									default: null
									unit:    "seconds"
								}
							}
						}
					}
				}
			}

			if features.send.tls.enabled {
				tls: configuration._tls_connect & {_args: {
					can_enable:             features.send.tls.can_enable
					can_verify_certificate: features.send.tls.can_enable
					can_verify_hostname:    features.send.tls.can_verify_hostname
					enabled_default:        features.send.tls.enabled_default
				}}
			}
		}
	}

	how_it_works: {
		if features.buffer.enabled {
			if features.send != _|_ {
				if features.send.batch != _|_ {
					if features.send.batch.enabled {
						buffers_batches: {
							title: "Buffers & batches"
							body: #"""
									<SVG src="/optimized_svg/buffers-and-batches-serial_538_160.svg" />

									This component buffers & batches data as shown in the diagram above. You'll notice that Vector treats these concepts
									differently, instead of treating them as global concepts, Vector treats them
									as sink specific concepts. This isolates sinks, ensuring services disruptions
									are contained and delivery guarantees are honored.

									*Batches* are flushed when 1 of 2 conditions are met:

									1. The batch age meets or exceeds the configured `timeout_secs`.
									2. The batch size meets or exceeds the configured <% if component.options.batch.children.respond_to?(:max_size) %>`max_size`<% else %>`max_events`<% end %>.

									*Buffers* are controlled via the [`buffer.*`](#buffer) options.
									"""#
						}
					}
				}
			}

			if features.send == _|_ {
				buffers: {
					title: "Buffers"
					body: """
						<SVG src="/img/buffers.svg" />

						This component buffers events as shown in
						the diagram above. This helps to smooth out data processing if the downstream
						service applies backpressure. Buffers are controlled via the
						[`buffer.*`](#buffer) options.
						"""
				}
			}
		}

		if features.healthcheck.enabled {
			healthchecks: {
				title: "Health checks"
				body: """
					Health checks ensure that the downstream service is
					accessible and ready to accept data. This check is performed
					upon sink initialization. If the health check fails an error
					will be logged and Vector will proceed to start.
					"""
				sub_sections: [
					{
						title: "Require health checks"
						body: """
								If you'd like to exit immediately upon a health
								check failure, you can pass the
								`--require-healthy` flag:

								```bash
								vector --config /etc/vector/vector.toml --require-healthy
								```
								"""
					},
					{
						title: "Disable health checks"
						body: """
								If you'd like to disable health checks for this
								sink you can set the `healthcheck` option to
								`false`.
								"""
					},
				]
			}
		}

		if features.send != _|_ {
			if features.send.request.enabled {
				partitioning: _ | *{
					title: "Partitioning"
					body: """
						Vector supports dynamic configuration values through a simple
						template syntax. If an option supports templating, it will be
						noted with a badge and you can use event fields to create dynamic
						values. For example:

						```toml title="vector.toml"
						[sinks.my-sink]
							dynamic_option = "application={{ application_id }}"
						```

						In the above example, the `application_id` for each event will be
						used to partition outgoing data.
						"""
				}
			}
		}

		if features.send != _|_ {
			if features.send.request.enabled {
				rate_limits: {
					title: "Rate limits & adapative concurrency"
					body:  null
					sub_sections: [
						{
							title: "Adaptive Request Concurrency (ARC)"
							body:  """
								Adaptive Requst Concurrency is a feature of Vector that does away
								with static rate limits and automatically optimizes HTTP
								concurrency limits based on downstream service responses. The
								underlying mechanism is a feedback loop inspired by TCP congestion
								control algorithms. Checkout the [announcement blog post](\(urls.adaptive_request_concurrency_post)),

								We highly recommend enabling this feature as it improves
								performance and reliability of Vector and the systems it
								communicates with.

								To enable, set the `request.concurrency` option to `adaptive`:

								```toml title="vector.toml"
								[sinks.my-sink]
								  request.concurrency = "adaptive"
								```
								"""
						},
						{
							title: "Static rate limits"
							body: """
								If Adaptive Request Concurrency is not for you, you can manually
								set static rate limits with the `request.rate_limit_duration_secs`,
								`request.rate_limit_num`, and `request.concurrency` options:

								```toml title="vector.toml"
								[sinks.my-sink]
								  request.rate_limit_duration_secs = 1
								  request.rate_limit_num = 10
								  request.concurrency = 10
								```
								"""
						},
					]
				}

				retry_policy: {
					title: "Retry policy"
					body: """
						Vector will retry failed requests (status == 429, >= 500, and != 501).
						Other responses will not be retried. You can control the number of
						retry attempts and backoff rate with the `request.retry_attempts` and
						`request.retry_backoff_secs` options.
						"""
				}
			}
		}

		if features.send != _|_ {
			if features.send.tls.enabled {
				transport_layer_security: {
					title: "Transport Layer Security (TLS)"
					body:  """
						Vector uses [Openssl](\(urls.openssl)) for TLS protocols for it's
						maturity. You can enable and adjust TLS behavior via the `tls.*`
						options.
						"""
				}
			}
		}
	}

	telemetry: metrics: {
		events_in_total:  components.sources.internal_metrics.output.metrics.events_in_total
		events_out_total: components.sources.internal_metrics.output.metrics.events_out_total
	}
}
