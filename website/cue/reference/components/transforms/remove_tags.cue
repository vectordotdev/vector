package metadata

components: transforms: remove_tags: {
	title: "Remove Tags"

	description: """
		Removes one or more metric tags.
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
		requirements: []
		warnings: [
			"""
			\(remove_tags._remap_deprecation_notice)

			```coffee
			del(.tag)
			```
			""",
		]
		notices: []
	}

	configuration: {
		tags: {
			description: "The tag names to drop."
			required:    true
			warnings: []
			type: array: items: type: string: {
				examples: ["tag1", "tag2"]
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
