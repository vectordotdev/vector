package metadata

remap: functions: is_metric: {
	arguments: [
	]
	internal_failure_reasons: []
	return: {
		types: ["boolean"]
		rules: [
			"If the current event is a [`metric` event](\(urls.vector_metric)), then `true` is returned."#,
		]
	}
	category: "Event"
	description: """
		Determines whether the current event is a [`metric` event](\(urls.vector_metric)).
		"""
	examples: [
		{
			title: "A metric event"
			source: """
				is_metric()
				"""
			return: true
		},
	]
}
