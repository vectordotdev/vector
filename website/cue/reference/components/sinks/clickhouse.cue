package metadata

components: sinks: clickhouse: {
	title: "Clickhouse"

	classes: {
		commonly_used: true
		delivery:      "at_least_once"
		development:   "beta"
		egress_method: "batch"
		service_providers: ["Yandex"]
		stateful: false
	}

	features: {
		buffer: enabled:      true
		healthcheck: enabled: true
		send: {
			batch: {
				enabled:      true
				common:       false
				max_bytes:    10485760
				timeout_secs: 1
			}
			compression: {
				enabled: true
				default: "gzip"
				algorithms: ["none", "gzip"]
				levels: ["none", "fast", "default", "best", 0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
			}
			encoding: {
				enabled: true
				codec: enabled: false
			}
			proxy: enabled: true
			request: {
				enabled: true
				headers: false
			}
			tls: {
				enabled:                true
				can_enable:             false
				can_verify_certificate: true
				can_verify_hostname:    true
				enabled_default:        false
			}
			to: {
				service: services.clickhouse

				interface: {
					socket: {
						api: {
							title: "Clickhouse HTTP API"
							url:   urls.clickhouse_http
						}
						direction: "outgoing"
						protocols: ["http"]
						ssl: "optional"
					}
				}
			}
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

		requirements: [
			"""
				[Clickhouse](\(urls.clickhouse)) version `>= 1.1.54378` is required.
				""",
		]
		warnings: []
		notices: []
	}

	configuration: {
		auth: configuration._http_auth & {_args: {
			password_example: "${CLICKHOUSE_PASSWORD}"
			username_example: "${CLICKHOUSE_USERNAME}"
		}}
		database: {
			common:      true
			description: "The database that contains the table that data will be inserted into."
			required:    false
			type: string: {
				examples: ["mydatabase"]
			}
		}
		endpoint: {
			description: "The endpoint of the [Clickhouse](\(urls.clickhouse)) server."
			required:    true
			type: string: {
				examples: ["http://localhost:8123"]
			}
		}
		table: {
			description: "The table that data will be inserted into."
			required:    true
			type: string: {
				examples: ["mytable"]
			}
		}
		skip_unknown_fields: {
			common:      true
			description: "Sets `input_format_skip_unknown_fields`, allowing Clickhouse to discard fields not present in the table schema."
			required:    false
			type: bool: default: false
		}
	}

	input: {
		logs:    true
		metrics: null
	}

	telemetry: metrics: {
		component_sent_bytes_total:       components.sources.internal_metrics.output.metrics.component_sent_bytes_total
		component_sent_events_total:      components.sources.internal_metrics.output.metrics.component_sent_events_total
		component_sent_event_bytes_total: components.sources.internal_metrics.output.metrics.component_sent_event_bytes_total
		events_out_total:                 components.sources.internal_metrics.output.metrics.events_out_total
	}
}
