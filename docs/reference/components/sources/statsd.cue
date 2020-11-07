package metadata

components: sources: statsd: {
	_port: 8125

	title:       "StatsD"
	description: "[StatsD](\(urls.statsd)) is a standard and, by extension, a set of tools that can be used to send, collect, and aggregate custom metrics from any application. Originally, StatsD referred to a daemon written by [Etsy](\(urls.etsy)) in Node."

	classes: {
		commonly_used: false
		delivery:      "best_effort"
		deployment_roles: ["aggregator"]
		development:   "stable"
		egress_method: "stream"
	}

	features: {
		multiline: enabled: false
		receive: {
			from: {
				name:     "StatsD"
				thing:    "a \(name) client"
				url:      urls.statsd
				versions: null

				interface: socket: {
					api: {
						title: "StatsD"
						url:   urls.statsd_udp_protocol
					}
					port: _port
					protocols: ["udp"]
					ssl: "optional"
				}
			}

			tls: enabled: false
		}
	}

	support: {
		platforms: {
			"aarch64-unknown-linux-gnu":  true
			"aarch64-unknown-linux-musl": true
			"x86_64-apple-darwin":        true
			"x86_64-pc-windows-msv":      true
			"x86_64-unknown-linux-gnu":   true
			"x86_64-unknown-linux-musl":  true
		}

		requirements: []
		warnings: []
		notices: []
	}

	configuration: {
		address: {
			description: "The address to listen for connections on, or `systemd#N` to use the Nth socket passed by systemd socket activation. If an address is used it _must_ include a port."
			groups: ["tcp", "udp"]
			required: true
			warnings: []
			type: string: {
				examples: ["0.0.0.0:\(_port)", "systemd", "systemd#3"]
			}
		}
		mode: {
			description: "The type of socket to use."
			groups: ["tcp", "udp", "unix"]
			required: true
			warnings: []
			type: string: {
				enum: {
					tcp:  "TCP Socket."
					udp:  "UDP Socket."
					unix: "Unix Domain Socket."
				}
			}
		}
		path: {
			description: "The unix socket path. *This should be an absolute path*."
			groups: ["unix"]
			required: true
			warnings: []
			type: string: {
				examples: ["/path/to/socket"]
			}
		}
		shutdown_timeout_secs: {
			common:      false
			description: "The timeout before a connection is forcefully closed during shutdown."
			groups: ["tcp"]
			required: false
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
		vector_connection_errors_total: _vector_connection_errors_total
		vector_invalid_record_total: {
			description: "The total number of invalid StatsD records discarded."
			type:        "counter"
			tags:        _component_tags
		}
		vector_invalid_record_bytes_total: {
			description: "The total number of bytes from StatsD journald records."
			type:        "counter"
			tags:        _component_tags
		}
	}
}
