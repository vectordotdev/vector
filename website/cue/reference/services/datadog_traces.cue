package metadata

services: datadog_traces: {
	name:     "Datadog traces"
	thing:    "a \(name) stream"
	url:      urls.datadog_traces
	versions: null

	description: services._datadog.description
}
