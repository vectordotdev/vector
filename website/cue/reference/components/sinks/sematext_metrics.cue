package metadata

components: sinks: sematext_metrics: {
	title: "Sematext Metrics"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		development:   "stable"
		service_providers: ["Sematext"]
		egress_method: "batch"
		stateful:      true
	}

	features: {
		acknowledgements: true
		auto_generated:   true
		healthcheck: enabled: true
		send: {
			batch: {
				enabled:      true
				common:       false
				max_events:   20
				timeout_secs: 1.0
			}
			compression: enabled: false
			encoding: {
				enabled: true
				codec: enabled: false
			}
			proxy: enabled:   true
			request: enabled: false
			tls: enabled:     false
			to: sinks._sematext.features.send.to
		}
	}

	support: {
		requirements: []
		warnings: [
			"""
				[Sematext monitoring](\(urls.sematext_monitoring)) only accepts metrics which contain a single value.
				Therefore, only `counter` and `gauge` metrics are supported. If you'd like to ingest other
				metric types please consider using the [`metric_to_log` transform](\(urls.vector_transforms)/metric_to_log)
				with the `sematext_logs` sink.
				""",
		]
		notices: []
	}

	configuration: base.components.sinks.sematext_metrics.configuration

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
		traces: false
	}
}
