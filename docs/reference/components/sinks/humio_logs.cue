package metadata

components: sinks: humio_logs: {
	title: "Humio Logs"

	description:   sinks._humio.description
	classes:       sinks._humio.classes
	features:      sinks._humio.features
	support:       sinks._humio.support
	configuration: sinks._humio.configuration

	input: {
		logs:    true
		metrics: null
	}
}
