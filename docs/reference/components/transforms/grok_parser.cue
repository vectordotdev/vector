package metadata

components: transforms: grok_parser: {
	title: "Grok Parser"

	description: """
		Parses a log field value with [Grok](\(urls.grok)).
		"""

	classes: {
		commonly_used: false
		development:   "deprecated"
		egress_method: "stream"
		stateful:      false
	}

	features: {
		parse: {
			format: {
				name:     "Grok"
				url:      urls.grok
				versions: null
			}
		}
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
			This component has been deprecated in favor of the new [`remap` transform's `parse_grok`
			function](\(urls.vector_remap_transform)#parse_grok)` The `remap` transform provides a
			simple syntax for robust data transformation. Let us know what you think!
			""",
		]
		notices: [
			"""
				Vector uses the Rust [`grok` library](\(urls.rust_grok_library)). All patterns
				[listed here](\(urls.grok_patterns)) are supported. It is recommended to use
				maintained patterns when possible since they will be improved over time by
				the community.
				""",
		]
	}

	configuration: {
		drop_field: {
			common:      true
			description: "If `true` will drop the specified `field` after parsing."
			required:    false
			warnings: []
			type: bool: default: true
		}
		field: {
			common:      true
			description: "The log field to execute the `pattern` against. Must be a `string` value."
			required:    false
			warnings: []
			type: string: {
				default: "message"
				examples: ["message", "parent.child", "array[0]"]
				syntax: "literal"
			}
		}
		pattern: {
			description: "The [Grok pattern](\(urls.grok_patterns))"
			required:    true
			warnings: []
			type: string: {
				examples: ["%{TIMESTAMP_ISO8601:timestamp} %{LOGLEVEL:level} %{GREEDYDATA:message}"]
				syntax: "literal"
			}
		}
		types: configuration._types
	}

	input: {
		logs:    true
		metrics: null
	}

	how_it_works: {
		available_patterns: {
			title: "Available Patterns"
			body:  support.notices[0]
		}

		testing: {
			title: "Testing"
			body:  """
				We recommend the [Grok debugger](\(urls.grok_debugger)) for Grok testing.
				"""
		}
	}

	telemetry: metrics: {
		processing_errors_total: components.sources.internal_metrics.output.metrics.processing_errors_total
	}
}
