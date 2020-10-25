package metadata

remap: functions: now: {
	arguments: []
	category: "text"
	return: ["timestamp"]
	description: #"""
			Returns the current timestamp in the Utc timezone.
		"""#
	examples: [
		{
			title: "Success"
			input: {}
			source: #"""
				.timestamp = format_timestamp(now(), format = "%v %R")
				"""#
			output: {
				timestamp: "21-Oct-2020 20:53"
			}
		},
	]
}
