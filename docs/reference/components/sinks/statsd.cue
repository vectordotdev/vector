package metadata

components: sinks: statsd: {
	title:             "Statsd"
	short_description: "Streams metric events to [StatsD][urls.statsd] metrics service."
	long_description:  "[StatsD][urls.statsd] is a standard and, by extension, a set of tools that can be used to send, collect, and aggregate custom metrics from any application. Originally, StatsD referred to a daemon written by [Etsy][urls.etsy] in Node."

	classes:  sinks.socket.classes
	features: sinks.socket.features
	support:  sinks.socket.support

	input: {
		logs: false
		metrics: {
			counter:      true
			distribution: true
			gauge:        true
			histogram:    true
			set:          true
			summary:      true
		}
	}

	configuration: sinks.socket.configuration & {
		namespace: {
			common:      true
			description: "A prefix that will be added to all metric names."
			required:    false
			warnings: []
			type: string: {
				default: null
				examples: ["service"]
			}
		}
	}
}
