package metadata

components: sinks: redis: {
	title: "Redis"
	classes: {
		commonly_used: false
		delivery:      "best_effort"
		development:   "beta"
		egress_method: "batch"
		service_providers: []
		stateful: false
	}
	features: {
		buffer: enabled:      true
		healthcheck: enabled: true
		send: {
			compression: enabled: false
			batch: {
				enabled:      true
				common:       true
				max_bytes:    null
				max_events:   1
				timeout_secs: 1
			}
			encoding: {
				enabled: true
				codec: {
					enabled: true
					enum: ["json", "text"]
				}
			}
			request: {
				enabled:     true
				concurrency: 1
				headers:     false
			}
			tls: {
				enabled:                true
				can_enable:             true
				can_verify_certificate: false
				can_verify_hostname:    false
				enabled_default:        false
			}
			to: {
				service: services.redis
				interface: {
					socket: {
						direction: "outgoing"
						protocols: ["tcp"]
						ssl: "optional"
					}
				}
			}
		}
	}

	support: {
		requirements: []
		warnings: []
		notices: []
	}

	configuration: {
		url: {
			description: "The Redis URL to connect to. The url _must_ take the form of `protocol://server:port/db` where the protocol can either be `redis` or `rediss` for connections secured via TLS."
			groups: ["tcp"]
			required: true
			type: string: {
				examples: ["redis://127.0.0.1:6379/0"]
			}
		}
		key: {
			description: "The Redis key to publish messages to."
			required:    true
			type: string: {
				examples: ["syslog:{{ app }}", "vector"]
				syntax: "template"
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
			}
		}
		list: {
			common:      false
			description: "Options for the Redis `list` data type."
			required:    false
			type: object: {
				examples: []
				options: {
					method: {
						common:      false
						description: "The method (`lpush` or `rpush`) to publish messages when `data_type` is list."
						required:    false
						type: string: {
							default: "rpush"
							enum: {
								lpush: "Use the `lpush` method to publish messages."
								rpush: "Use the `rpush` method to publish messages."
							}
						}
					}
				}
			}
		}
	}

	input: {
		logs:    true
		metrics: null
	}

	how_it_works: {
		redis_rs: {
			title: "redis-rs"
			body:  """
				The `redis` sink uses [`redis-rs`](\(urls.redis_rs)) under the hood, which is a high level Redis library
				for Rust. It provides convenient access to all Redis functionality through a very flexible but low-level
				API.
				"""
		}
	}

	telemetry: metrics: {
		component_sent_events_total:      components.sources.internal_metrics.output.metrics.component_sent_events_total
		component_sent_event_bytes_total: components.sources.internal_metrics.output.metrics.component_sent_event_bytes_total
		events_in_total:                  components.sources.internal_metrics.output.metrics.events_in_total
		events_out_total:                 components.sources.internal_metrics.output.metrics.events_out_total
		send_errors_total:                components.sources.internal_metrics.output.metrics.send_errors_total
		processed_bytes_total:            components.sources.internal_metrics.output.metrics.processed_bytes_total
		processed_events_total:           components.sources.internal_metrics.output.metrics.processed_events_total
	}
}
