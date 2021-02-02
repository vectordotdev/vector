package metadata

components: sources: statsd: {
	_port: 8125

	title: "StatsD"

	classes: {
		commonly_used: false
		delivery:      "best_effort"
		deployment_roles: ["aggregator"]
		development:   "stable"
		egress_method: "stream"
		stateful:      false
	}

	features: {
		multiline: enabled: false
		receive: {
			from: {
				service: services.statsd
				interface: socket: {
					api: {
						title: "StatsD"
						url:   urls.statsd_udp_protocol
					}
					direction: "incoming"
					port:      _port
					protocols: ["udp"]
					ssl: "optional"
				}
			}
			receive_buffer_bytes: {
				enabled:       true
				relevant_when: "mode = `tcp` or mode = `udp` && os = `unix`"
			}
			keepalive: enabled: true
			tls: enabled:       false
		}
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
		address: {
			description:   "The address to listen for connections on, or `systemd#N` to use the Nth socket passed by systemd socket activation. If an address is used it _must_ include a port."
			relevant_when: "mode = `tcp` or `udp`"
			required:      true
			warnings: []
			type: string: {
				examples: ["0.0.0.0:\(_port)", "systemd", "systemd#3"]
				syntax: "literal"
			}
		}
		mode: {
			description: "The type of socket to use."
			required:    true
			warnings: []
			type: string: {
				enum: {
					tcp:  "TCP Socket."
					udp:  "UDP Socket."
					unix: "Unix Domain Socket."
				}
				syntax: "literal"
			}
		}
		path: {
			description:   "The unix socket path. *This should be an absolute path*."
			relevant_when: "mode = `unix`"
			required:      true
			warnings: []
			type: string: {
				examples: ["/path/to/socket"]
				syntax: "literal"
			}
		}
		shutdown_timeout_secs: {
			common:        false
			description:   "The timeout before a connection is forcefully closed during shutdown."
			relevant_when: "mode = `tcp`"
			required:      false
			warnings: []
			type: uint: {
				default: 30
				unit:    "seconds"
			}
		}

	}

	output: metrics: {
		counter:      output._passthrough_counter
		distribution: output._passthrough_distribution
		gauge:        output._passthrough_gauge
		set:          output._passthrough_set
	}

	how_it_works: {
		timestamps: {
			title: "Timestamps"
			body: """
				StatsD protocol does not provide support for sending metric
				timestamps. You'll notice that each parsed metric is assigned a
				`null` timestamp, which is a special value which means "a real
				time metric", i.e. not a historical one. Normally such `null`
				timestamps will be substituted by current time by downstream
				sinks or 3rd party services during sending/ingestion. See the
				[metric][docs.data-model.metric] data model page for more info.
				"""
		}
	}

	telemetry: metrics: {
		connection_errors_total:    components.sources.internal_metrics.output.metrics.connection_errors_total
		invalid_record_total:       components.sources.internal_metrics.output.metrics.invalid_record_total
		invalid_record_bytes_total: components.sources.internal_metrics.output.metrics.invalid_record_bytes_total
		processed_bytes_total:      components.sources.internal_metrics.output.metrics.processed_bytes_total
		processed_events_total:     components.sources.internal_metrics.output.metrics.processed_events_total
	}
}
