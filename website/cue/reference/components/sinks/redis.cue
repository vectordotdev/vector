package metadata

components: sinks: redis: {
	title: "Redis"
	classes: {
		commonly_used: false
		delivery:      "best_effort"
		development:   "stable"
		egress_method: "batch"
		service_providers: []
		stateful: false
	}
	features: {
		auto_generated:   true
		acknowledgements: true
		healthcheck: enabled: true
		send: {
			compression: enabled: false
			batch: {
				enabled:      true
				common:       true
				max_bytes:    null
				max_events:   1
				timeout_secs: 1.0
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
			tls: enabled: false
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

	configuration: base.components.sinks.redis.configuration

	input: {
		logs:    true
		metrics: null
		traces:  false
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
		send_errors_total: components.sources.internal_metrics.output.metrics.send_errors_total
	}
}
