package metadata

components: transforms: add_tags: {
	title:       "Add Tags"
	description: "Adds tags to metric events."

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
		requirements: []
		warnings: [
			"""
			\(add_tags._remap_deprecation_notice)

			```coffee
			#".tag = "value""#
			```
			""",
		]
		notices: []
	}

	configuration: {
		overwrite: {
			common:      true
			description: "By default, fields will be overridden. Set this to `false` to avoid overwriting values."
			required:    false
			warnings: []
			type: bool: default: true
		}
		tags: {
			common:      true
			description: "A table of key/value pairs representing the tags to be added to the metric."
			required:    false
			warnings: []
			type: object: {
				examples: [
					{
						"static_tag": "my value"
						"env_tag":    "${ENV_VAR}"
					},
				]
				options: {}
			}
		}
	}

	input: {
		logs: false
		metrics: {
			counter:      true
			distribution: true
			gauge:        true
			histogram:    true
			set:          true
			summary:      true
		}
	}
}
