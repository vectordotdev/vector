package metadata

components: transforms: remove_fields: {
	title: "Remove Fields"

	description: """
		Removes one or more log fields.
		"""

	classes: {
		commonly_used: false
		development:   "deprecated"
		egress_method: "stream"
		stateful:      false
	}

	features: {
		shape: {}
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
			This component has been deprecated in favor of the new [`remap` transform's `del`
			function](\(urls.vector_remap_transform)#del). The `remap` transform provides a simple
			syntax for robust data transformation. Let us know what you think!
			""",
		]
		notices: []
	}

	configuration: {
		drop_empty: {
			common:      false
			description: "If set to `true`, after removing fields, remove any parent objects that are now empty."
			required:    false
			warnings: []
			type: bool: default: false
		}
		fields: {
			description: "The log field names to drop."
			required:    true
			warnings: []
			type: array: items: type: string: {
				examples: ["field1", "field2", "parent.child"]
				syntax: "literal"
			}
		}
	}

	input: {
		logs:    true
		metrics: null
	}
}
