package metadata

components: transforms: ansi_stripper: {
	title: "ANSI Stripper"

	description: """
		Strips [ANSI escape sequences](\(urls.ansi_escape_codes)).
		"""

	classes: {
		commonly_used: false
		development:   "deprecated"
		egress_method: "stream"
		stateful:      false
	}

	features: {
		sanitize: {}
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
		warnings: [
			"""
			\(ansi_stripper._remap_deprecation_notice)

			```vrl
			.message = strip_ansi_escape_codes(.message)
			```
			""",
		]
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
				syntax: "literal"
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
