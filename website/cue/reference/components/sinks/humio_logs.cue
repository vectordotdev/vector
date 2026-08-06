package metadata

components: sinks: humio_logs: {
	title: "Humio Logs"

	_humio_encoding: {
		enabled: true
		codec: {
			enabled: true
			enum: ["json", "text"]
		}
	}

	classes:       sinks._humio.classes
	features:      sinks._humio.features
	support:       sinks._humio.support
	configuration: generated.components.sinks.humio_logs.configuration

	input: {
		logs:    true
		metrics: null
		traces:  false
	}
}
