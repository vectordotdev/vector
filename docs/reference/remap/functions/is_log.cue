package metadata

remap: functions: is_log: {
	arguments: [
	]
	internal_failure_reasons: []
	return: {
		types: ["boolean"]
		rules: [
			"If the current event is a [`log` event](\(urls.vector_log)), then `true` is returned.",
		]
	}
	category:    "Event"
	description: """
		Determines whether the current event is a [`log` event](\(urls.vector_log)).
		"""
	examples: [
		{
			title: "A log event"
			input: log: message: "Hello, world!"
			source: """
				is_log()
				"""
			return: true
		},
	]
}
