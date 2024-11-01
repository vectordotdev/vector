package metadata

remap: functions: uuid_v7: {
	category:    "Random"
	description: """
		Generates a random [UUIDv7](\(urls.uuidv7)) string.
		"""

	arguments: [
		{
			name:        "timestamp"
			description: "The timestamp used to generate the UUIDv7."
			required:    false
			type: ["timestamp"]
			default: "`now()`"
		},
	]
	internal_failure_reasons: []
	return: types: ["string"]

	examples: [
		{
			title: "Create a UUIDv7 with implicit `now()`"
			source: #"""
				uuid_v7()
				"""#
			return: "06338364-8305-7b74-8000-de4963503139"
		},
		{
			title: "Create a UUIDv7 with explicit `now()`"
			source: #"""
				uuid_v7(now())
				"""#
			return: "018e29b3-0bea-7f78-8af3-d32ccb1b93c1"
		},
		{
			title: "Create a UUIDv7 with custom timestamp"
			source: #"""
				uuid_v7(t'2020-12-30T22:20:53.824727Z')
				"""#
			return: "0176b5bd-5d19-7394-bb60-c21028c6152b"
		},
	]
}
