package metadata

components: sinks: humio_metrics: {
	title: "Humio Metrics"

	classes:       sinks._humio.classes
	features:      sinks._humio.features
	support:       sinks._humio.support
	configuration: base.components.sinks.humio_metrics.configuration

	input: {
		logs: false
		metrics: {
			counter:      true
			distribution: true
			gauge:        true
			histogram:    true
			set:          true
			summary:      true
		}
		traces: false
	}

	how_it_works: {
		metrics: {
			title: "Metrics"
			body: """
				Metrics are converted to log events via the `log_to_event` transform prior to sending to humio.
				"""
		}
	}
}
