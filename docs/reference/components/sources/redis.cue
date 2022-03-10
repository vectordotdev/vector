package metadata

components: sources: redis: {
	title: "Redis"

	features: {
		acknowledgements: false
		collect: {
			checkpoint: enabled: false
			tls: {
				enabled:                true
				can_enable:             true
				can_verify_certificate: false
				can_verify_hostname:    false
				enabled_default:        false
			}
			from: {
				service: services.redis
				interface: {
					socket: {
						direction: "incoming"
						port:      6379
						protocols: ["tcp"]
						ssl: "disabled"
					}
				}
			}
		}
		multiline: enabled: false
		codecs: {
			enabled:         true
			default_framing: "bytes"
		}
	}

	classes: {
		commonly_used: false
		delivery:      "best_effort"
		deployment_roles: ["aggregator"]
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

		requirements: []
		warnings: []
		notices: []
	}

	installation: {
		platform_name: null
	}

	configuration: {
		url: {
			description: "The Redis URL to connect to. The url _must_ take the form of `protocol://server:port/db` where the protocol can either be `redis` or `rediss` for connections secured via TLS."
			groups: ["tcp"]
			required: true
			warnings: []
			type: string: {
				examples: ["redis://127.0.0.1:6379/0"]
				syntax: "literal"
			}
		}
		key: {
			description: "The Redis key to read messages from."
			required:    true
			warnings: []
			type: string: {
				examples: ["vector"]
				syntax: "literal"
			}
		}
		data_type: {
			common:      false
			description: "The Redis data type (`list` or `channel`) to use."
			required:    false
			type: string: {
				default: "list"
				enum: {
					list:    "Use the Redis `list` data type."
					channel: "Use the Redis `channel` data type."
				}
				syntax: "literal"
			}
		}
		list: {
			common:      false
			description: "Options for the Redis `list` data type."
			required:    false
			warnings: []
			type: object: {
				examples: []
				options: {
					method: {
						common:      false
						description: "The method (`rpop` or `lpop`) to pop messages when `data_type` is list."
						required:    false
						type: string: {
							default: "lpop"
							enum: {
								lpop: "Pop messages from the head of the list."
								rpop: "Pop messages from the tail of the list."
							}
							syntax: "literal"
						}
					}
				}
			}
		}
		redis_key: {
			common:      false
			description: "The log field name to use for the redis key. If set to an empty string or null, the key is not added to the log event."
			required:    false
			warnings: []
			type: string: {
				default: "redis_key"
				examples: ["redis_key"]
				syntax: "literal"
			}
		}
	}

	output: logs: record: {
		description: "An individual Redis record"
		fields: {
			host:      fields._local_host
			message:   fields._raw_line
			timestamp: fields._current_timestamp
			redis_key: {
				description: "The Redis key the event came from"
				required:    false
				type: string: {}
			}
		}
	}

	how_it_works: {
		redis_rs: {
			title: "redis-rs"
			body:  """
				The `redis` source uses [`redis-rs`](\(urls.redis_rs)) under the hood, which is a high level Redis library
				for Rust. It provides convenient access to all Redis functionality through a very flexible but low-level
				API.
				"""
		}
	}

	telemetry: metrics: {
		events_in_total:         components.sources.internal_metrics.output.metrics.events_in_total
		events_out_total:        components.sources.internal_metrics.output.metrics.events_out_total
		processed_bytes_total:   components.sources.internal_metrics.output.metrics.processed_bytes_total
		processed_bytes_total:   components.sources.internal_metrics.output.metrics.processed_bytes_total
		processing_errors_total: components.sources.internal_metrics.output.metrics.processing_errors_total
	}
}
