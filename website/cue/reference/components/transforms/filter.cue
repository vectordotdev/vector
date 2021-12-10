package metadata

components: transforms: filter: {
	title: "Filter"

	description: """
		Filters events based on a set of conditions.
		"""

	classes: {
		commonly_used: true
		development:   "stable"
		egress_method: "stream"
		stateful:      false
	}

	features: {
		filter: {}
	}

	support: {
		requirements: []
		warnings: []
		notices: []
	}

	configuration: {
		condition: {
			description: """
				The condition to be matched against every input event. Only messages that pass the condition are
				forwarded; messages that don't pass are dropped.
				"""
			required: true
			type: condition: {}
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

	examples: [
		{
			title: "Drop debug logs"
			configuration: {
				condition: #".level != "debug""#
			}
			input: [
				{
					log: {
						level:   "debug"
						message: "I'm a noisy debug log"
					}
				},
				{
					log: {
						level:   "info"
						message: "I'm a normal info log"
					}
				},
			]
			output: [
				{
					log: {
						level:   "info"
						message: "I'm a normal info log"
					}
				},
			]
		},
	]

	telemetry: metrics: {
		events_discarded_total: components.sources.internal_metrics.output.metrics.events_discarded_total
	}
}
