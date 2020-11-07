package metadata

components: sinks: console: {
	title: "Console"

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
			encoding: {
				enabled: true
				codec: {
					enabled: true
					default: null
					enum: ["json", "text"]
				}
			}
			request: enabled: false
			tls: enabled:     false
			to: {
				name:     "STDOUT"
				thing:    "a \(name) stream"
				url:      urls.stdout
				versions: null

				interface: stdout: {}
			}
		}
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
		target: {
			common:      true
			description: "The [standard stream](\(urls.standard_streams)) to write to."
			required:    false
			warnings: []
			type: string: {
				default: "stdout"
				enum: {
					stdout: "Output will be written to [STDOUT](\(urls.stdout))"
					stderr: "Output will be written to [STDERR](\(urls.stderr))"
				}
			}
		}
	}

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
	}

	telemetry: metrics: {
		vector_processing_errors_total: _vector_processing_errors_total
	}
}
