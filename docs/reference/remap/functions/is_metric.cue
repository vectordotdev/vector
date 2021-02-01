package metadata

remap: functions: is_metric: {
	arguments: [
	]
	internal_failure_reasons: []
	return: {
		types: ["boolean"]
		rules: [
			"If the current event is a [`metric` event](\(urls.vector_metric)), then `true` is returned.",
		]
	}
	category:    "Event"
	description: """
		Determines whether the current event is a [`metric` event](\(urls.vector_metric)).
		"""
	examples: [
		{
			title: "A metric event"
			input: metric: {
				kind: "incremental"
				name: "user_login_total"
				counter: value: 102.0
				tags: host:     "my.host.com"
			}
			source: """
				is_metric()
				"""
			return: true
		},
	]
}
