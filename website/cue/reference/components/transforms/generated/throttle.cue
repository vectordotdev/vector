package metadata

generated: components: transforms: throttle: configuration: {
	exclude: {
		description: "A condition that lets matching events pass without consuming bucket capacity."
		required:    false
		type: condition: {}
	}
	internal_metrics: {
		description: "Configuration of internal metrics for the Throttle transform."
		required:    false
		type: object: options: {
			emit_events_discarded_per_key: {
				description: """
					Whether or not to emit the `events_discarded_total` internal metric with the `key` tag.

					If true, the counter will be incremented for each discarded event, including the key value
					associated with the discarded event. If false, the counter will not be emitted. Instead, the
					number of discarded events can be seen through the `component_discarded_events_total` internal
					metric.

					Note that this defaults to false because the `key` tag has potentially unbounded cardinality.
					Only set this to true if you know that the number of unique keys is bounded.
					"""
				required: false
				type: bool: default: false
			}
			include_group_tag: {
				description: """
					Whether or not to include the `group` tag on the `component_discarded_events_total`
					internal metric.

					When enabled, adds a `group` tag containing the rendered `key_field` value.
					If the `key_field` value is missing or can't be rendered, the tag value is `None`.

					Note that this defaults to false because the `group` tag has potentially unbounded
					cardinality. Only set this to true if you know that the number of unique groups is bounded.
					"""
				required: false
				type: bool: default: false
			}
		}
	}
	key_field: {
		description: """
			A template whose rendered value groups events into independently rate-limited buckets.

			Events use a shared default bucket when the template is omitted or cannot be rendered.
			"""
		required: false
		type: string: {
			examples: ["{{ service }}", "{{ hostname }}"]
			syntax: "template"
		}
	}
	threshold: {
		description: """
			The default number of events allowed per bucket per `window_secs`.

			A positive threshold is required even when `threshold_field` is set. It applies to
			events whose threshold field is missing or invalid.
			"""
		required: true
		type: uint: {}
	}
	threshold_field: {
		description: """
			The literal path to the event or metadata field that supplies the threshold for each event.

			Use `rate_limit` for an event field or `%rate_limit` for a metadata field. The value
			must be an integer from 1 to 4,294,967,295 or a string that parses as one. Missing
			fields and invalid values, including floats, use the configured `threshold`.
			Events in the same bucket share capacity even when their thresholds differ.
			"""
		required: false
		type: string: examples: ["rate_limit"]
	}
	window_secs: {
		description: "The time in seconds for a bucket to replenish its full capacity."
		required:    true
		type: float: unit: "seconds"
	}
}
