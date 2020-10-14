package metadata

components: sinks: file: {
	title:             "File"
	short_description: "Streams log events to a file."
	long_description:  "Streams log events to a file."

	classes: {
		commonly_used: false
		function:      "transmit"
		service_providers: []
	}

	features: {
		batch: enabled:  false
		buffer: enabled: false
		compression: {
			enabled: true
			default: null
			gzip:    true
		}
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

	support: {
		input_types: ["log"]

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
		idle_timeout_secs: {
			common:      false
			description: "The amount of time a file can be idle  and stay open. After not receiving any events for this timeout, the file will be flushed and closed.\n"
			required:    false
			warnings: []
			type: uint: {
				default: 30
				unit:    null
			}
		}
		path: {
			description: "File name to write events to."
			required:    true
			warnings: []
			type: string: {
				examples: ["/tmp/vector-%Y-%m-%d.log", "/tmp/application-{{ application_id }}-%Y-%m-%d.log"]
			}
		}
	}
}
