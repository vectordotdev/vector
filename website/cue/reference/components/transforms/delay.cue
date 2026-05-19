package metadata

components: transforms: delay: {
	title: "Delay events"

	description: """
		Delays events by a set duration.
		"""

	classes: {
		commonly_used: false
		development:   "stable"
		egress_method: "stream"
		stateful:      true
	}

	features: {
		filter: {}
	}

	support: {
		requirements: []
		warnings: []
		notices: []
	}

	configuration: generated.components.transforms.delay.configuration

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

	output: {
		logs: "": {
			description: "The input `log` event."
		}
		metrics: "": {
			description: "The input `metric` event."
		}
		traces: "": {
			description: "The input `trace` event."
		}
	}
}
