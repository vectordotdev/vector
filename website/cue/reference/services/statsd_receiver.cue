package metadata

services: statsd_receiver: {
	name:     "StatsD receiver"
	thing:    "a \(name)"
	url:      urls.statsd
	versions: null

	description: "[StatsD](\(urls.statsd)) is a standard and, by extension, a set of tools that can be used to send, collect, and aggregate custom metrics from any application. Originally, StatsD referred to a daemon written by [Etsy](\(urls.etsy)) in Node."
}
