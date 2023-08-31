package metadata

components: sources: redis: {
	title: "Redis"

	features: {
		auto_generated:   true
		acknowledgements: false
		collect: {
			checkpoint: enabled: false
			tls: enabled:        false
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
		development:   "stable"
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

	configuration: base.components.sources.redis.configuration

	output: logs: record: {
		description: "An individual Redis record"
		fields: {
			host:      fields._local_host
			message:   fields._raw_line
			timestamp: fields._current_timestamp
			source_type: {
				description: "The name of the source type."
				required:    true
				type: string: {
					examples: ["redis"]
				}
			}
			redis_key: {
				description: "The Redis key the event came from"
				required:    false
				common:      false
				type: string: {
					examples: ["some_key"]
					default: null
				}
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
}
