package metadata

services: prometheus: {
	name:     "Prometheus"
	thing:    "a \(name) database"
	url:      urls.prometheus
	versions: null

	description: "[Prometheus](\(urls.prometheus)) is a pull-based monitoring system that scrapes metrics from configured endpoints, stores them efficiently, and supports a powerful query language to compose dynamic information from a variety of otherwise unrelated data points."
}
