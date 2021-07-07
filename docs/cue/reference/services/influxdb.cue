package metadata

services: influxdb: {
	name:     "InfluxDB"
	thing:    "an \(name) database"
	url:      urls.influxdb
	versions: null

	description: "[InfluxDB](\(urls.influxdb)) is an open-source time series database developed by InfluxData. It is written in Go and optimized for fast, high-availability storage and retrieval of time series data in fields such as operations monitoring, application metrics, Internet of Things sensor data, and real-time analytics."
}
