remap: concepts: event: {
	title:       "Event"
	description: """
		VRL programs exist to operate on events, therefore, events are the primary subject in VRL programs. In the
		context of Vector, events map to [Vector's events](\(urls.vector_data_model)). For example, the following
		VRL program adds a field to a log event:

		```vrl
		.new_field = "new value"
		```

		The current event is the context of the VRL program.
		"""

	characteristics: {
		path: {
			title:       "Paths"
			description: """
				Of particular note for events are [path expressions](\(urls.vrl_path_expressions)). Because events
				represent the program context, paths refer to values within the event:

				```vrl
				.kubernetes.pod_id
				```
				"""
		}
	}
}
