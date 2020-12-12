package metadata

services: datadog_logs: {
	name:     "Datadog logs"
	thing:    "a \(name) index"
	url:      urls.datadog_logs
	versions: null

	description: services._datadog.description
}
