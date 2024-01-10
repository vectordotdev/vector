package metadata

components: transforms: sample: {
	title: "Sample"

	description: """
		Samples events at a configurable rate.
		"""

	classes: {
		commonly_used: false
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

	configuration: base.components.transforms.sample.configuration

	input: {
		logs:    true
		metrics: null
		traces:  true
	}
}
