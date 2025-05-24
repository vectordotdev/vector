package metadata

components: sinks: aws_xray: components._aws & {
	title: "AWS X-Ray"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		development:   "beta"
		egress_method: "stream"
		service_providers: ["AWS"]
		stateful: false
	}

	features: {
		acknowledgements: true
		auto_generated:   true
	}

	support: {
		requirements: []
		warnings: []
		notices: []
	}

	configuration: base.components.sinks.aws_xray.configuration & {
		_aws_include: false
	}

	input: {
		logs:    true
		metrics: null
		traces:  false
	}
}
