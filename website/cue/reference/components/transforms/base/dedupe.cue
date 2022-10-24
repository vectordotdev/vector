package metadata

base: components: transforms: dedupe: configuration: {
	cache: {
		description: "Caching configuration for deduplication."
		required:    false
		type: object: options: num_events: {
			description: "Number of events to cache and use for comparing incoming events to previously seen events."
			required:    true
			type: uint: default: 5000
		}
	}
	fields: {
		description: """
			Options to control what fields to match against.

			When no field matching configuration is specified, events are matched using the `timestamp`,
			 `host`, and `message` fields from an event. The specific field names used will be those set in
			 the global [`log schema`][global_log_schema] configuration.

			[global_log_schema]: https://vector.dev/docs/reference/configuration/global-options/#log_schema
			"""
		required: true
		type: object: {
			examples: ["field1", "parent.child_field", "host", "hostname"]
			options: {
				ignore: {
					required: true
					type: array: items: type: string: syntax: "literal"
				}
				match: {
					required: true
					type: array: items: type: string: syntax: "literal"
				}
			}
		}
	}
}
