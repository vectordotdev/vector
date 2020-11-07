package metadata

components: transforms: ansi_stripper: {
	title: "ANSI Stripper"

	classes: {
		commonly_used: false
		development:   "stable"
		egress_method: "stream"
	}

	features: {
		sanitize: {}
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
		field: {
			common:      true
			description: "The target field to strip ANSI escape sequences from."
			required:    false
			warnings: []
			type: string: {
				default: "message"
				examples: ["message", "parent.child", "array[0]"]
			}
		}
	}

	input: {
		logs:    true
		metrics: null
	}

	telemetry: metrics: {
		vector_processing_errors_total: _vector_processing_errors_total
	}
}
