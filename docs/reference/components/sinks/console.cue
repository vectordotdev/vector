package metadata

components: sinks: console: {
	title:             "Console"
	short_description: "Streams log and metric events to [standard output streams][urls.standard_streams], such as [STDOUT][urls.stdout] and [STDERR][urls.stderr]."
	long_description:  "Streams log and metric events to [standard output streams][urls.standard_streams], such as [STDOUT][urls.stdout] and [STDERR][urls.stderr]."

	classes: {
		commonly_used: false
		function:      "test"
		service_providers: []
	}

	features: {
		batch: {
			enabled:      false
			common:       false
			max_bytes:    30000000
			max_events:   null
			timeout_secs: 1
		}
		buffer: enabled:      false
		compression: enabled: false
		encoding: {
			enabled: true
			default: null
			json:    null
			ndjson:  null
			text:    null
		}
		healthcheck: enabled: true
		request: enabled:     false
		tls: enabled:         false
	}

	statuses: {
		delivery:    "at_least_once"
		development: "stable"
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
		notices: []
	}

	configuration: {
		target: {
			common:      true
			description: "The [standard stream][urls.standard_streams] to write to."
			required:    false
			warnings: []
			type: string: {
				default: "stdout"
				enum: {
					stdout: "Output will be written to [STDOUT][urls.stdout]"
					stderr: "Output will be written to [STDERR][urls.stderr]"
				}
			}
		}
	}
}
