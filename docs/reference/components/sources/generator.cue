package metadata

components: sources: generator: {
	title: "Generator"

	description: """
		Generates fakes events, useful for testing, benchmarking, and demoing.
		"""

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		deployment_roles: ["daemon", "sidecar"]
		development:   "stable"
		egress_method: "stream"
	}

	features: {
		multiline: enabled: false
		generate: {}
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

	installation: {
		platform_name: null
	}

	configuration: {
		batch_interval: {
			common:      false
			description: "The amount of time, in seconds, to pause between each batch of output lines. If not set, there will be no delay."
			required:    false
			warnings: []
			type: float: {
				default: null
				examples: [1.0]
			}
		}
		count: {
			common:      false
			description: "The number of times to repeat outputting the `lines`. By default the source will continuously print logs (infinite)."
			required:    false
			warnings: []
			type: uint: {
				default: null
				unit:    null
			}
		}
		lines: {
			description: "The list of lines to output."
			required:    true
			warnings: []
			type: array: items: type: string: examples: ["Line 1", "Line 2"]
		}
		sequence: {
			common:      false
			description: "If `true`, each output line will start with an increasing sequence number."
			required:    false
			warnings: []
			type: bool: default: false
		}
	}

	output: logs: {}
}
