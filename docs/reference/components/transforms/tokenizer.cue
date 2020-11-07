package metadata

components: transforms: tokenizer: {
	title:       "Tokenizer"
	description: "Tokenizes a field's value by splitting on white space, ignoring special wrapping characters, and zip the tokens into ordered field names."

	classes: {
		commonly_used: true
		development:   "stable"
		egress_method: "stream"
	}

	features: {
		parse: {
			format: {
				name:     "Token Format"
				url:      null
				versions: null
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
		drop_field: {
			common:      true
			description: "If `true` the `field` will be dropped after parsing."
			required:    false
			warnings: []
			type: bool: default: true
		}
		field: {
			common:      true
			description: "The log field to tokenize."
			required:    false
			warnings: []
			type: string: {
				default: "message"
				examples: ["message", "parent.child"]
			}
		}
		field_names: {
			description: "The log field names assigned to the resulting tokens, in order."
			required:    true
			warnings: []
			type: array: items: type: string: examples: ["timestamp", "level", "message", "parent.child"]
		}
		types: configuration._types
	}

	examples: [
		{
			title: "Loosely Structured"
			configuration: {
				field: "message"
				field_names: ["remote_addr", "ident", "user_id", "timestamp", "message", "status", "bytes"]
				types: {
					timestamp: "timestamp"
					status:    "int"
					bytes:     "int"
				}
			}
			input: log: {
				message: #"5.86.210.12 - zieme4647 [19/06/2019:17:20:49 -0400] "GET /embrace/supply-chains/dynamic/vertical" 201 20574"#
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

	input: {
		logs:    true
		metrics: null
	}

	how_it_works: {
		blank_values: {
			title: "Blank Values"
			body: #"""
				Both `" "` and `"-"` are considered blank values and their mapped fields will
				be set to `null`.
				"""#
		}

		special_characters: {
			title: "Special Characters"
			body: #"""
				In order to extract raw values and remove wrapping characters, we must treat
				certain characters as special. These characters will be discarded:

				* `"..."` - Quotes are used tp wrap phrases. Spaces are preserved, but the wrapping quotes will be discarded.
				* `[...]` - Brackets are used to wrap phrases. Spaces are preserved, but the wrapping brackets will be discarded.
				* `\` - Can be used to escape the above characters, Vector will treat them as literal.
				"""#
		}
	}

	telemetry: metrics: {
		vector_processing_errors_total: _vector_processing_errors_total
	}
}
