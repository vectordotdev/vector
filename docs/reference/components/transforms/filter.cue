package metadata

components: transforms: filter: {
	title: "Filter"

	classes: {
		commonly_used: true
		development:   "stable"
		egress_method: "stream"
	}

	features: {
		filter: {}
	}

	support: {
		targets: {
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
		condition: {
			description: "The set of logical conditions to be matched against every input event. Only messages that pass all conditions will be forwarded."
			required:    true
			warnings: []
			type: object: configuration._conditions
		}
	}

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
	}

	telemetry: metrics: {
		events_discarded_total: components.sources.internal_metrics.output.metrics.events_discarded_total
	}

	examples: [
		{
			title: "Drop debug logs"
			configuration: {
				condition: "level.neq": "debug"
			}
			input: [
				{log: {
					level:   "debug"
					message: "I'm a noisy debug log"
				}},
				{log: {
					level:   "info"
					message: "I'm a normal info log"
				}},
			]
			output: [
				{log: {
					level:   "info"
					message: "I'm a normal info log"
				}},
			]
		},
	]
}
