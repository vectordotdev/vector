package metadata

components: transforms: ansi_stripper: {
	title: "ANSI Stripper"

	description: """
		Strips [ANSI escape sequences](\(urls.ansi_escape_codes)).
		"""

	classes: {
		commonly_used: false
		development:   "stable"
		egress_method: "stream"
	}

	features: {
		sanitize: {}
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
		warnings: [transforms.add_fields.support.warnings[0]]
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
		processing_errors_total: components.sources.internal_metrics.output.metrics.processing_errors_total
	}
}
