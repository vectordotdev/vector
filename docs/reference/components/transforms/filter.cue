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
		condition: {
			description: "The set of logical conditions to be matched against every input event. Only messages that pass all conditions will be forwarded."
			required:    true
			warnings: []
			type: object: configuration._conditions
		}
	}

	input: {
		logs:    true
		metrics: null
	}
}
