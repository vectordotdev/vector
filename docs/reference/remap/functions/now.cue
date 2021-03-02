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
			return: "2020-10-21T20:53:12.212221Z"
		},
	]
}
