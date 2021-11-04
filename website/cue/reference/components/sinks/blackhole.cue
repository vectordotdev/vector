package metadata

components: sinks: blackhole: {
	title: "Blackhole"

	classes: {
		commonly_used: false
		delivery:      "best_effort"
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
			encoding: enabled:    false
			request: enabled:     false
			tls: enabled:         false
		}
	}

	support: {
		requirements: []
		warnings: []
		notices: []
	}

	configuration: {
		print_interval_secs: {
			common:      false
			description: "The number of seconds between reporting a summary of activity."
			required:    false
			type: uint: {
				default: 1
				examples: [10]
				unit: "seconds"
			}
		}
		rate: {
			common:      false
			description: "Rates the amount of events that the sink can consume per second."
			required:    false
			type: uint: {
				default: null
				examples: [1000]
				unit: null
			}
		}
	}

	input: {
		logs:    true
		metrics: null
	}

	telemetry: metrics: {
		processed_bytes_total:  components.sources.internal_metrics.output.metrics.processed_bytes_total
		processed_events_total: components.sources.internal_metrics.output.metrics.processed_events_total
	}
}
