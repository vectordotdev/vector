package metadata

remap: functions: uuid_v4: {
	arguments: []
	return: ["string"]
	category: "text"
	description: #"""
		Returns a random UUID (Universally Unique Identifier).
		"""#
	examples: [
		{
			title: "Success"
			input: {}
			source: #"""
				.id = uuid_v4()
				"""#
			output: {
				id: "1d262f4f-199b-458d-879f-05fd0a5f0683"
			}
		},
	]
}
