package metadata

components: sinks: _datadog: {
	classes: {
		commonly_used: false
		delivery:      "at_least_once"
		development:   "stable"
		egress_method: "batch"
		service_providers: ["Datadog"]
		stateful: false
	}

	support: {
		targets: {
			"aarch64-unknown-linux-gnu":      true
			"aarch64-unknown-linux-musl":     true
			"armv7-unknown-linux-gnueabihf":  true
			"armv7-unknown-linux-musleabihf": true
			"x86_64-apple-darwin":            true
			"x86_64-pc-windows-msv":          true
			"x86_64-unknown-linux-gnu":       true
			"x86_64-unknown-linux-musl":      true
		}
		requirements: []
		warnings: []
		notices: []
	}

	configuration: {
		api_key: {
			description: "Datadog [API key](https://docs.datadoghq.com/api/?lang=bash#authentication)"
			required:    true
			warnings: []
			type: string: {
				examples: ["${DATADOG_API_KEY_ENV_VAR}", "ef8d5de700e7989468166c40fc8a0ccd"]
				syntax: "literal"
			}
		}
		endpoint: {
			common:        false
			description:   "The endpoint to send data to."
			relevant_when: "region is not set"
			required:      false
			type: string: {
				default: null
				examples: ["127.0.0.1:8080", "example.com:12345"]
				syntax: "literal"
			}
		}
		region: {
			description:   "The region to send data to."
			required:      false
			relevant_when: "endpoint is not set"
			warnings: []
			type: string: {
				enum: {
					us: "United States"
					eu: "Europe"
				}
				syntax: "literal"
			}
		}
	}
}
