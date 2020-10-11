package metadata

components: sinks: vector: {
	title:             "Vector"
	short_description: "Streams log and metric events to another downstream [`vector` source][docs.sources.vector]."
	long_description:  "Streams log and metric events to another downstream [`vector` source][docs.sources.vector]."

	classes: {
		commonly_used: false
		function:      "transmit"
		service_providers: []
	}

	features: {
		batch: enabled:       false
		buffer: enabled:      true
		compression: enabled: false
		encoding: enabled:    false
		healthcheck: enabled: true
		request: enabled:     false
		tls: {
			enabled:                true
			can_enable:             true
			can_verify_certificate: true
			can_verify_hostname:    true
			enabled_default:        false
		}
	}

	statuses: {
		delivery:    "best_effort"
		development: "beta"
	}

	support: {
		input_types: ["log", "metric"]

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
	}

	configuration: {
		address: {
			description: "The downstream Vector address to connect to. The address _must_ include a port."
			required:    true
			warnings: []
			type: string: {
				examples: ["92.12.333.224:5000"]
			}
		}
	}
}
