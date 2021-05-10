remap: concepts: event: {
	title:       "Event"
	description: """
		VRL programs operate on observability [events](\(urls.vector_data_model)). This VRL program, for example, adds
		a field to a log event:

		```vrl
		.new_field = "new value"
		```

		The event, `.`, at hand is the entire context of the VRL program.

		The event can be set to a value other than an object; for example (`. = 5`). If it is set to an array, each
		element of that array will be emitted as its own event from the `remap` function. For any elements that are not
		an object, or if the top-level `.` is set to a scalar value, that value will be set as the `message` key on the
		emitted object.

		For example:

		```vrl
		. = ["hello", 1, true, { "foo": "bar" }]
		```

		Will result in the following four events being emitted:

		```json
		{ "message": "hello" }
		{ "message": 1 }
		{ "message": true }
		{ "foo": "bar" }
		```
		"""

	characteristics: {
		path: {
			title:       "Paths"
			description: """
				[Path expressions](\(urls.vrl_path_expressions)) enable you to access values inside the event:

				```vrl
				.kubernetes.pod_id
				```
				"""
		}
	}
}
