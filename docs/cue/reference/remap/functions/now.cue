package metadata

remap: functions: now: {
	category: "Timestamp"
	description: """
		Returns the current timestamp in the UTC timezone with nanosecond precision.
		"""

	arguments: []
	internal_failure_reasons: []
	return: types: ["timestamp"]

	examples: [
		{
			title: "Generate a current timestamp"
			source: #"""
				now()
				"""#
			return: "2021-03-04T10:51:15.928937Z"
		},
	]
}
