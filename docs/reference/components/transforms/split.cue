package metadata

components: transforms: split: {
	title: "Split"

	classes: {
		commonly_used: false
		development:   "stable"
		egress_method: "stream"
	}

	features: {
		shape: {}
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
		drop_field: {
			common:      true
			description: "If `true` the `field` will be dropped after parsing."
			required:    false
			warnings: []
			type: bool: default: true
		}
		field: {
			common:      true
			description: "The field to apply the split on."
			required:    false
			warnings: []
			type: string: {
				default: "message"
				examples: ["message", "parent.child"]
			}
		}
		field_names: {
			description: "The field names assigned to the resulting tokens, in order."
			required:    true
			warnings: []
			type: array: items: type: string: examples: ["timestamp", "level", "message", "parent.child"]
		}
		separator: {
			common:      true
			description: "The separator to split the field on. If no separator is given, it will split on all whitespace. 'Whitespace' is defined according to the terms of the [Unicode Derived Core Property `White_Space`](\(urls.unicode_whitespace))."
			required:    false
			warnings: []
			type: string: {
				default: "[whitespace]"
				examples: [","]
			}
		}
		types: configuration._types
	}

	input: {
		logs:    true
		metrics: null
	}

	examples: [
		{
			title: "Split log message"
			configuration: {
				field:     "message"
				separator: ","
				field_names: ["remote_addr", "user_id", "timestamp", "message", "status", "bytes"]
				types: {
					status: "int"
					bytes:  "int"
				}
			}
			input: log: {
				message: "5.86.210.12,zieme4647,19/06/2019:17:20:49 -0400,GET /embrace/supply-chains/dynamic/vertical,201,20574"
			}
			output: log: {
				remote_addr: "5.86.210.12"
				user_id:     "zieme4647"
				timestamp:   "19/06/2019:17:20:49 -0400"
				message:     "GET /embrace/supply-chains/dynamic/vertical"
				status:      201
				bytes:       20574
			}
		},
	]

	telemetry: metrics: {
		vector_processing_errors_total: _vector_processing_errors_total
	}
}
