package metadata

services: datadog_metrics: {
	name:     "Datadog metrics"
	thing:    "a \(name) database"
	url:      urls.datadog_metrics
	versions: null

	description: services._datadog.description
}
