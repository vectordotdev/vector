package metadata

remap: functions: uuid_v4: {
	category:    "Random"
	description: """
		Generates a random [UUIDv4](\(urls.uuidv4)) string.
		"""

	arguments: []
	internal_failure_reasons: []
	return: types: ["string"]

	examples: [
		{
			title: "Create a UUIDv4"
			source: #"""
				uuid_v4()
				"""#
			return: "1d262f4f-199b-458d-879f-05fd0a5f0683"
		},
	]
}
