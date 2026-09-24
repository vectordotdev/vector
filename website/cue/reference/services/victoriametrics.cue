package metadata

services: victoriametrics: {
	name:     "VictoriaMetrics"
	thing:    "a \(name) database"
	url:      urls.victoriametrics
	versions: null

	description: "[VictoriaMetrics](\(urls.victoriametrics)) is a fast, cost-effective, and scalable time series database and monitoring solution. It accepts data through many protocols, including its own remote write protocol, and supports the PromQL-compatible MetricsQL query language."
}
