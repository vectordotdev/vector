package metadata

components: transforms: swimlanes: {
	title: "Swimlanes"

	classes: {
		commonly_used: false
		development:   "beta"
		egress_method: "stream"
	}

	features: {
		route: {}
	}

	support: {
		platforms: {
			"aarch64-unknown-linux-gnu":  true
			"aarch64-unknown-linux-musl": true
			"x86_64-apple-darwin":        true
			"x86_64-pc-windows-msv":      true
			"x86_64-unknown-linux-gnu":   true
			"x86_64-unknown-linux-musl":  true
		}

		requirements: []
		warnings: []
		notices: []
	}

	configuration: {
		lanes: {
			description: "A table of swimlane identifiers to logical conditions representing the filter of the swimlane. Each swimlane can then be referenced as an input by other components with the name `<transform_name>.<swimlane_id>`."
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
				lanes: {
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
		vector_events_discarded_total: _vector_events_discarded_total
	}
}
