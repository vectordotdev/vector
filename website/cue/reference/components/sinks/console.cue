package metadata

components: sinks: console: {
	title: "Console"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		development:   "stable"
		egress_method: "stream"
		service_providers: []
		stateful: false
	}

	features: {
		buffer: enabled:      false
		healthcheck: enabled: false
		send: {
			compression: enabled: false
			encoding: {
				enabled: true
				codec: {
					enabled: true
					enum: ["json", "text", "ndjson"]
				}
			}
			request: enabled: false
			tls: enabled:     false
			to: {
				service: services.stdout
				interface: stdout: {}
			}
		}
	}

	support: {
		requirements: []
		warnings: []
		notices: []
	}

	configuration: {
		target: {
			common:      true
			description: "The [standard stream](\(urls.standard_streams)) to write to."
			required:    false
			type: string: {
				default: "stdout"
				enum: {
					stdout: "Output will be written to [STDOUT](\(urls.stdout))"
					stderr: "Output will be written to [STDERR](\(urls.stderr))"
				}
			}
		}
	}

	input: {
		logs: true
		metrics: {
			counter:      true
			distribution: true
			gauge:        true
			histogram:    true
			set:          true
			summary:      true
		}
	}

	telemetry: metrics: {
		processed_bytes_total:   components.sources.internal_metrics.output.metrics.processed_bytes_total
		processed_events_total:  components.sources.internal_metrics.output.metrics.processed_events_total
		processing_errors_total: components.sources.internal_metrics.output.metrics.processing_errors_total
	}
}
