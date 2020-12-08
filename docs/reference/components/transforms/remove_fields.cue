package metadata

components: transforms: remove_fields: {
	title: "Remove Fields"

	classes: {
		commonly_used: false
		development:   "stable"
		egress_method: "stream"
	}

	features: {
		shape: {}
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
		warnings: []
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
			type: array: items: type: string: examples: ["field1", "field2", "parent.child"]
		}
	}

	input: {
		logs:    true
		metrics: null
	}
}
