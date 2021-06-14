package metadata

services: loki: {
	name:     "Loki"
	thing:    "a \(name) database"
	url:      urls.loki
	versions: null

	description: "[Loki](\(urls.loki)) is a horizontally-scalable, highly-available, multi-tenant log aggregation system inspired by [Prometheus](\(urls.prometheus)). It is designed to be very cost effective and easy to operate. It does not index the contents of the logs, but rather a set of labels for each log stream."
}
