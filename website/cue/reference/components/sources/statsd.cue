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
				relevant_when: "mode = `tcp` or mode = `udp`"
			}
			keepalive: enabled: true
			tls: enabled:       false
		}
	}

	support: {
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
			type: string: {
				examples: ["0.0.0.0:\(_port)", "systemd", "systemd#3"]
			}
		}
		mode: {
			description: "The type of socket to use."
			required:    true
			type: string: {
				enum: {
					tcp:  "TCP Socket."
					udp:  "UDP Socket."
					unix: "Unix Domain Socket."
				}
			}
		}
		path: {
			description:   "The unix socket path. *This should be an absolute path*."
			relevant_when: "mode = `unix`"
			required:      true
			type: string: {
				examples: ["/path/to/socket"]
			}
		}
		shutdown_timeout_secs: {
			common:        false
			description:   "The timeout before a connection is forcefully closed during shutdown."
			relevant_when: "mode = `tcp`"
			required:      false
			type: uint: {
				default: 30
				unit:    "seconds"
			}
		}
		connection_limit: {
			common:        false
			description:   "The max number of TCP connections that will be processed."
			relevant_when: "mode = `tcp`"
			required:      false
			type: uint: {
				default: null
				unit:    "concurrency"
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
			body:  """
				The StatsD protocol doesn't provide support for sending metric timestamps. You may
				notice that each parsed metric is assigned a `null` timestamp, which is a special
				value indicating a realtime metric (i.e. not a historical metric). Normally, such
				`null` timestamps are substituted with the current time by downstream sinks or
				third-party services during sending/ingestion. See the
				[metric data model](\(urls.vector_metric)) page for more info.
				"""
		}
	}

	telemetry: metrics: {
		events_in_total:                 components.sources.internal_metrics.output.metrics.events_in_total
		connection_errors_total:         components.sources.internal_metrics.output.metrics.connection_errors_total
		invalid_record_total:            components.sources.internal_metrics.output.metrics.invalid_record_total
		invalid_record_bytes_total:      components.sources.internal_metrics.output.metrics.invalid_record_bytes_total
		processed_bytes_total:           components.sources.internal_metrics.output.metrics.processed_bytes_total
		processed_events_total:          components.sources.internal_metrics.output.metrics.processed_events_total
		component_received_events_total: components.sources.internal_metrics.output.metrics.component_received_events_total
	}
}
