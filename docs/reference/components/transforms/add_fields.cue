package metadata

components: transforms: add_fields: {
	title:             "#{component.title}"
	short_description: "Accepts log events and allows you to add one or more log fields."
	long_description:  "Accepts log events and allows you to add one or more log fields."

	_features: {
		checkpoint: enabled: false
		multiline: enabled:  false
		tls: enabled:        false
	}

	classes: {
		commonly_used: false
		function:      "schema"
	}

	statuses: {
		development: "stable"
	}

	support: {
		input_types: ["log"]

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
		fields: {
			description: "A table of key/value pairs representing the keys to be added to the event."
			required:    true
			warnings: []
			type: object: {
				examples: [
					{
						string_field:    "string value"
						env_var_field:   "${ENV_VAR}"
						templated_field: "{{ my_other_field }}"
						int_field:       1
						float_field:     1.2
						bool_field:      true
						timestamp_field: "1979-05-27 00:32:00 -0700"
						parent: child_field: "child_value"
						list_field: ["first", "second", "third"]
					},
				]
				options: {
					"*": {
						description: "The name of the field to add. Accepts all [supported types][docs.configuration#types]. Use `.` for adding nested fields."
						required:    true
						warnings: []
						type: "*": {}
					}
				}
			}
		}
		overwrite: {
			common:      true
			description: "By default, fields will be overridden. Set this to `false` to avoid overwriting values.\n"
			required:    false
			warnings: []
			type: bool: default: true
		}
	}

	how_it_works: {
		conflicts: {
			title: "Conflicts"
			body:  ""
			sub_sections: [
				{
					title: "Key Conflicts"
					body: #"""
						Keys specified in this transform will replace existing keys.
						"""#
				},
				{
					title: "Nested Key Conflicts"
					body: #"""
						Nested keys are added in a _deep_ fashion. They will not replace any ancestor
						objects. For example, given the following `log` event:

						```javascript
						{
						  "parent": {
						    "child1": "value1"
						  }
						}
						```

						And the following configuration:

						```toml
						[transforms.add_nested_field]
						  type = "add_fields"
						  fields.parent.child2 = "value2"
						```

						Will result in the following event:

						```javascript
						{
						  "parent": {
						    "child1": "value1",
						    "child2": "value2"
						  }
						}
						```

						Notice that `parent.child1` field was preserved.
						"""#
				},
			]
		}
		types: {
			title: "Types"
			body: #"""
				All supported [configuration value types][docs.configuration#types] are
				accepted. This includes primitive types (`string`, `int`, `float`, `boolean`)
				and special types, such as [arrays](#arrays) and
				[nested fields](#nested-fields).
				"""#
		}
	}
}
