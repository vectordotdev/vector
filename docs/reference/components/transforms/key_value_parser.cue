package metadata

components: transforms: key_value_parser: {
	title: "Key Value Parser"

	classes: {
		commonly_used: false
		development:   "stable"
		egress_method: "stream"
	}

	features: {
		parse: {
			format: {
				name:     "Key Value"
				url:      urls.key_value
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
		warnings: [
			"""
				Performance characteristics of the `key_value` transform have not been benchmarked.
			""",
		]
		notices: [
			"""
				It is likely that the `key_value` transform will replace the `logfmt` transform in the future since
				it offers a more flexible super-set of that transform.
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
			description: "The log field containing key/value pairs to parse. Must be a `string` value."
			required:    false
			warnings: []
			type: string: {
				default: "message"
				examples: ["message", "parent.child", "array[0]"]
			}
		}
		field_split: {
			common: 	 true
			description: "The character(s) to split a key/value pair on which results in a new field with an associated value. Must be a `string` value."
			required:    false
			type: string: {
				default: "="
				examples: [":", "="]
			}
		}

		overwrite_target: {
			description: """
				If `target_field` is set and the log contains a field of the same name
				as the target, it will only be overwritten if this is set to `true`.
			"""
			required: 	false
			type: string: {
				default: false
			}
		}

		separator: {
			description: "The character(s) that separate key/value pairs. Must be a `string` value."
			required: 	false
			type: string: {
				default: "[whitespace]"
				examples: [",", ";", "|"]
			}
		}

		target_field {
			description: """
				If this setting is present, the parsed JSON will be inserted into the
				log as a sub-object with this name.
				If a field with the same name already exists, the parser will fail and
				produce an error.
			"""
			type: string: {
				examples: ["root_field", "parent.child"]
			}
		}

		trim_key {
			description: """
				Removes characters from the beginning and end of a key until a character that is not listed.
				ex: `<key>=value` would result in `key: value` with this option set to `<>`.
			"""
			type: string: {
				examples: ["<>", "{}"]
			}
		}

		trim_value {
			description: """
				Removes characters from the beginning and end of a value until a character that is not listed.\
				ex: `key=<<>value>>` would result in `key: value` with this option set to `<>`.\
			"""
			type: string: {
				examples: ["<>", "{}"]
			}
		}

		types: configuration._types
	}

	input: {
		logs:    true
		metrics: null
	}

	how_it_works: {
		stuff: "Not sure what to put here... you give it `key:value; k:val` it extracts them to `json`. Pretty straight forward."
	}


}
