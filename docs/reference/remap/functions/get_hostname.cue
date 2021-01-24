package metadata

remap: functions: get_hostname: {
	arguments: []
	internal_failure_reasons: []
	return: types: ["string"]
	category: "System"
	description: #"""
		Get system's hostname.
		"""#
	examples: [
		{
			title: "Get hostname"
			input: log: {}
			source: #"""
				.hostname = get_hostname!()
				"""#
			output: log: hostname: "localhost.localdomain"
		},
	]
}
