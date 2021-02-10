remap: concepts: event: {
	title:       "Event"
	description: """
		VRL programs operate on observability [events](\(urls.vector_data_model)). This VRL program, for example, adds
		a field to a log event:

		```vrl
		.new_field = "new value"
		```

		The event at hand is the entire context of the VRL program.
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
