package metadata

import (
	"list"
)

components: sinks: [Name=string]: {
	kind: "sink"

	configuration: {
		if sinks[Name].features.send != _|_ && sinks[Name].features.send.batch != _|_ {
			if sinks[Name].features.send.batch.enabled {
				batch: {
					common:      false
					description: "Configures the sink batching behavior."
					required:    false
					type: object: {
						examples: []
						options: {
							if sinks[Name].features.send.batch.max_bytes != null {
								max_bytes: {
									common:      true
									description: "The maximum size of a batch, in bytes, before it is flushed."
									required:    false
									type: uint: {
										default: sinks[Name].features.send.batch.max_bytes
										unit:    "bytes"
									}
								}
							}
							if sinks[Name].features.send.batch.max_events != null {
								max_events: {
									common:      true
									description: "The maximum size of a batch, in events, before it is flushed."
									required:    false
									type: uint: {
										default: sinks[Name].features.send.batch.max_events
										unit:    "events"
									}
								}
							}
							timeout_secs: {
								common:      true
								description: "The maximum age of a batch before it is flushed."
								required:    false
								type: uint: {
									default: sinks[Name].features.send.batch.timeout_secs
									unit:    "seconds"
								}
							}
						}
					}
				}
			}
		}

		if sinks[Name].features.buffer.enabled {
			buffer: {
				common:      false
				description: "Configures the sink specific buffer behavior."
				required:    false
				type: object: {
					examples: []
					options: {
						max_events: {
							common:      true
							description: "The maximum number of [events][docs.data-model] allowed in the buffer."
							required:    false
							type: uint: {
								default: 500
								unit:    "events"
							}
						}
						max_size: {
							description: "The maximum size of the buffer on the disk."
							required:    true
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
							}
						}
					}
				}
			}
		}

		if sinks[Name].features.send != _|_ {
			if sinks[Name].features.send.compression.enabled {
				compression: {
					common:      true
					description: "The compression strategy used to compress the encoded event data before transmission."
					required:    false
					type: string: {
						default: sinks[Name].features.send.compression.default
						enum: {
							if list.Contains(sinks[Name].features.send.compression.algorithms, "none") {
								none: "No compression."
							}
							if list.Contains(sinks[Name].features.send.compression.algorithms, "gzip") {
								gzip: "[Gzip](\(urls.gzip)) standard DEFLATE compression."
							}
						}
					}
				}
			}
		}

		if sinks[Name].features.send != _|_ {
			if sinks[Name].features.send.encoding.enabled {
				encoding: {
					description: "Configures the encoding specific sink behavior."
					required:    true
					type: object: options: {
						if sinks[Name].features.send.encoding.codec.enabled {
							codec: {
								description: "The encoding codec used to serialize the events before outputting."
								required:    true
								type: string: examples: sinks[Name].features.send.encoding.codec.enum
							}
						}

						if sinks[Name].features.healthcheck.enabled {except_fields: {
							common:      false
							description: "Prevent the sink from encoding the specified labels."
							required:    false
							type: array: {
								default: null
								items: type: string: examples: ["message", "parent.child"]
							}
						}

							only_fields: {
								common:      false
								description: "Prevent the sink from encoding the specified labels."
								required:    false
								type: array: {
									default: null
									items: type: string: examples: ["message", "parent.child"]
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
								}
							}
						}
					}
				}
			}
		}

		if sinks[Name].features.send != _|_ {
			healthcheck: {
				common:      true
				description: "Enables/disables the sink healthcheck upon Vector boot."
				required:    false
				type: bool: default: true
			}
		}

		if sinks[Name].features.send != _|_ {
			if sinks[Name].features.send.request.enabled {
				request: {
					common:      false
					description: "Configures the sink request behavior."
					required:    false
					type: object: {
						examples: []
						options: {
							in_flight_limit: {
								common:      true
								description: "The maximum number of in-flight requests allowed at any given time."
								required:    false
								type: uint: {
									default: sinks[Name].features.send.request.in_flight_limit
									unit:    "requests"
								}
							}
							rate_limit_duration_secs: {
								common:      true
								description: "The time window, in seconds, used for the `rate_limit_num` option."
								required:    false
								type: uint: {
									default: sinks[Name].features.send.request.rate_limit_duration_secs
									unit:    "seconds"
								}
							}
							rate_limit_num: {
								common:      true
								description: "The maximum number of requests allowed within the `rate_limit_duration_secs` time window."
								required:    false
								type: uint: {
									default: sinks[Name].features.send.request.rate_limit_num
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
									default: sinks[Name].features.send.request.retry_initial_backoff_secs
									unit:    "seconds"
								}
							}
							retry_max_duration_secs: {
								common:      false
								description: "The maximum amount of time, in seconds, to wait between retries."
								required:    false
								type: uint: {
									default: sinks[Name].features.send.request.retry_max_duration_secs
									unit:    "seconds"
								}
							}
							timeout_secs: {
								common:      true
								description: "The maximum time a request can take before being aborted. It is highly recommended that you do not lower value below the service's internal timeout, as this could create orphaned requests, pile on retries, and result in duplicate data downstream."
								required:    false
								type: uint: {
									default: sinks[Name].features.send.request.timeout_secs
									unit:    "seconds"
								}
							}
						}
					}
				}
			}
		}

		if sinks[Name].features.send != _|_ {
			if sinks[Name].features.send.tls.enabled {
				tls: configuration._tls & {_args: {
					can_enable:      sinks[Name].features.send.tls.can_enable
					enabled_default: sinks[Name].features.send.tls.enabled_default
				}}
			}
		}
	}

	how_it_works: {
		if !sinks[Name].features.healthcheck.enabled {
			healthchecks: {
				title: "Healthchecks"
				body: """
					Health checks ensure that the downstream service is
					accessible and ready to accept data. This check is performed
					upon sink initialization. You can disable this check by
					setting the `healthcheck` option to `false`.
					"""
			}
		}
	}
}
