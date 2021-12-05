package metadata

services: humio: {
	name:     "Humio"
	thing:    "a \(name) database"
	url:      urls.humio
	versions: null

	description: "[Humio](\(urls.humio)) is a time-series logging and aggregation platform for unrestricted, comprehensive event analysis, On-Premises or in the Cloud. With 1TB/day of raw log ingest/node, in-memory stream processing, and live, shareable dashboards and alerts, you can instantly and in real-time explore, monitor, and visualize any system’s data. Metrics are converted to log events via the metric_to_log transform."
}
