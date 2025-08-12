package metadata

generated: components: transforms: dedupe: configuration: {
	cache: {
		description: "Caching configuration for deduplication."
		required:    false
		type: object: options: num_events: {
			description: "Number of events to cache and use for comparing incoming events to previously seen events."
			required:    false
			type: uint: default: 5000
		}
	}
	fields: {
		description: """
			Options to control what fields to match against.

			When no field matching configuration is specified, events are matched using the `timestamp`,
			`host`, and `message` fields from an event. The specific field names used are those set in
			the global [`log schema`][global_log_schema] configuration.

			[global_log_schema]: https://vector.dev/docs/reference/configuration/global-options/#log_schema
			"""
		required: false
		type: object: options: {
			ignore: {
				description: "Matches events using all fields except for the ignored ones."
				required:    true
				type: array: items: type: string: examples: ["field1", "parent.child_field", "host", "hostname"]
			}
			match: {
				description: "Matches events using only the specified fields."
				required:    true
				type: array: items: type: string: examples: ["field1", "parent.child_field"]
			}
		}
	}
	time_settings: {
		description: "Configuration for time based cache."
		required:    false
		type: object: options: {
			max_age_ms: {
				description: "Maximum age of items in deduplication cache, before duplicates are allowed again."
				required:    true
				type: uint: unit: "milliseconds"
			}
			refresh_on_drop: {
				description: "Set to true if dropped duplicates should refresh the age timer."
				required:    false
				type: bool: default: false
			}
		}
	}
}
