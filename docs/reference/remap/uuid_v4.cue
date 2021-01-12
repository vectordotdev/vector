package metadata

remap: functions: uuid_v4: {
	arguments: []
	internal_failure_reasons: []
	return: ["string"]
	category: "Random"
	description: #"""
		Generates a random [UUIDv4](\(urls.uuidv4)) string.
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
