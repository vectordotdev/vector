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
	}

	features: {
		route: {}
	}

	support: {
		targets: {
			"aarch64-unknown-linux-gnu":      true
			"aarch64-unknown-linux-musl":     true
			"armv7-unknown-linux-gnueabihf":  true
			"armv7-unknown-linux-musleabihf": true
			"x86_64-apple-darwin":            true
			"x86_64-pc-windows-msv":          true
			"x86_64-unknown-linux-gnu":       true
			"x86_64-unknown-linux-musl":      true
		}
		requirements: []
		warnings: []
		notices: []
	}

	configuration: {
		route: {
			description: "A table of route identifiers to logical conditions representing the filter of the route. Each route can then be referenced as an input by other components with the name `<transform_name>.<route_id>`."
			required:    true
			warnings: []
			type: object: {
				options: {
					"*": {
						description: "test"
						required:    true
						warnings: []
						type: object: configuration._conditions
					}
				}
			}
		}
	}

	input: {
		logs:    true
		metrics: null
	}

	examples: [
		{
			title: "Split by log level"
			configuration: {
				route: {
					debug: "level.eq": "debug"
					info: "level.eq":  "info"
					warn: "level.eq":  "warn"
					error: "level.eq": "error"
				}
			}
			input: log: {
				level: "info"
			}
			output: log: {
				level: "info"
			}
		},
	]

	telemetry: metrics: {
		events_discarded_total: components.sources.internal_metrics.output.metrics.events_discarded_total
	}
}
