package metadata

components: sinks: sematext_metrics: {
	title:       "Sematext Metrics"
	description: "[Sematext](\(urls.sematext)) is a hosted monitoring platform for metrics based on InfluxDB. Providing powerful monitoring and management solutions to monitor and observe your apps in real-time."

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		development:   "beta"
		service_providers: ["Sematext"]
		egress_method: "batch"
	}

	features: {
		buffer: enabled:      true
		healthcheck: enabled: true
		send: {
			batch: {
				enabled:      true
				common:       false
				max_bytes:    30000000
				max_events:   null
				timeout_secs: 1
			}
			compression: enabled: false
			encoding: {
				enabled: true
				codec: enabled: false
			}
			request: enabled: false
			tls: enabled:     false
			to: sinks._sematext.features.send.to
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
		warnings: [
			"""
				[Sematext monitoring](\(urls.sematext_monitoring)) only accepts metrics which contain a single value.
				Therefore, only `counter` and `gauge` metrics are supported. If you'd like to ingest other
				metric types please consider using the [`metric_to_log` transform][docs.transforms.metric_to_log]
				with the `sematext_logs` sink.
				""",
		]
		notices: []
	}

	configuration: sinks._sematext.configuration & {
		default_namespace: {
			description: "Used as a namespace for metrics that don't have it."
			required:    true
			warnings: []
			type: string: {
				examples: ["service"]
			}
		}
	}

	input: {
		logs: false
		metrics: {
			counter:      true
			distribution: false
			gauge:        true
			histogram:    false
			set:          false
			summary:      false
		}
	}

	telemetry: metrics: {
		vector_processing_errors_total: _vector_processing_errors_total
	}
}
