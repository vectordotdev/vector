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
			This component has been deprecated in favor of the new [`remap` transform's `strip_ansi_escape_codes`
			function](\(urls.vector_remap_transform)#strip_ansi_escape_codes). The `remap` transform provides a
			simple syntax for robust data transformation. Let us know what you think!
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
