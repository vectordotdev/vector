package metadata

remap: functions: is_log: {
	arguments: [
	]
	internal_failure_reasons: []
	return: {
		types: ["boolean"]
		rules: [
			#"If the current event is a log event, then `true` is returned."#,
		]
	}
	category: "Type"
	description: #"""
		Determines whether the current event is a log event.
		"""#
	examples: [
		{
			title: "A log event"
			source: """
				is_log()
				"""
			return: true
		},
	]
}
