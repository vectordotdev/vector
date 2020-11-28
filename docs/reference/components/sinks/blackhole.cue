package metadata

components: sinks: blackhole: {
	title: "Blackhole"

	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		development:   "stable"
		egress_method: "stream"
		service_providers: []
	}

	features: {
		buffer: enabled:      false
		healthcheck: enabled: false
		send: {
			compression: enabled: false
			encoding: enabled:    false
			request: enabled:     false
			tls: enabled:         false
		}
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
		print_amount: {
			common:      false
			description: "The number of events that must be received in order to print a summary of activity."
			required:    false
			warnings: []
			type: uint: {
				default: 1000
				examples: [1000]
				unit: null
			}
		}
	}

	input: {
		logs:    true
		metrics: null
	}
}
