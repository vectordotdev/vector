package metadata

components: transforms: route: {
	title: "Route"

	description: """
		Splits a stream of events into multiple sub-streams based on a set of
		conditions.
		"""

	classes: {
		commonly_used: false
		development:   "stable"
		egress_method: "stream"
		stateful:      false
	}

	features: {
		route: {}
	}

	support: {
		requirements: []
		warnings: []
		notices: []
	}

	configuration: base.components.transforms.route.configuration

	input: {
		logs: true
		metrics: {
			counter:      true
			distribution: true
			gauge:        true
			histogram:    true
			set:          true
			summary:      true
		}
		traces: true
	}

	examples: [
		{
			title: "Split by log level"

			configuration: {
				route: {
					debug: #".level == "debug""#
					info:  #".level == "info""#
					warn:  #".level == "warn""#
					error: #".level == "error""#
				}
			}

			input: log: {
				level: "info"
			}
			output: log: {
				level: "info"
			}
		},
		{
			title: "Split by metric namespace"

			configuration: {
				route: {
					app:  #".namespace == "app""#
					host: #".namespace == "host""#
				}
			}

			input: metric: {
				counter: {
					value: 10000.0
				}
				kind:      "absolute"
				name:      "memory_available_bytes"
				namespace: "host"
			}
			output: metric: {
				counter: {
					value: 10000.0
				}
				kind:      "absolute"
				name:      "memory_available_bytes"
				namespace: "host"
			}
		},
	]

	outputs: [
		{
			name:        "<route_id>"
			description: "Each route can be referenced as an input by other components with the name `<transform_name>.<route_id>`."
		},
	]
}
