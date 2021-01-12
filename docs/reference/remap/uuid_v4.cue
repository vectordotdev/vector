package metadata

remap: functions: uuid_v4: {
	arguments: []
	return: ["string"]
	category: "Random"
	description: #"""
		Returns a random UUID (Universally Unique Identifier).
		"""#
	examples: [
		{
			title: "Create UUIDv4"
			input: log: {}
			source: #"""
				.id = uuid_v4()
				"""#
			output: log: id: "1d262f4f-199b-458d-879f-05fd0a5f0683"
		},
	]
}
