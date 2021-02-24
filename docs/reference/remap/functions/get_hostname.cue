package metadata

remap: functions: get_hostname: {
	category: "System"
	description: """
		Gets the local system's hostname.
		"""

	arguments: []
	internal_failure_reasons: []
	return: types: ["string"]

	examples: [
		{
			title: "Get hostname"
			input: log: {}
			source: #"""
				.hostname = get_hostname()
				"""#
			output: log: hostname: "localhost.localdomain"
		},
	]
}
