package metadata

remap: functions: now: {
	arguments: []
	category: "Timestamp"
	return: ["timestamp"]
	description: #"""
		Returns the current timestamp in the Utc timezone.
		"""#
	examples: [
		{
			title: "Success"
			input: {}
			source: #"""
				.timestamp = now()
				"""#
			output: {
				timestamp: "21-Oct-2020 20:53"
			}
		},
	]
}
