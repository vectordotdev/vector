package metadata

remap: functions: exists: {
	arguments: [
		{
			name:        "path"
			description: "The paths of the fields to check."
			required:    true
			multiple:    false
			type: ["path"]
		},
	]
	internal_failure_reasons: []
	return: ["boolean"]
	category: "Event"
	description: #"""
		Checks if the given `path` exists. Nested paths and arrays can also be checked.
		"""#
	examples: [
		{
			title: "Exists (field)"
			input: log: field: 1
			source: #"""
				exists(.field)
				"""#
			return: true
		},
		{
			title: "Exits (array element)"
			input: log: array: [1, 2, 3]
			source: #"""
				exists(.array[2])
				"""#
			return: true
		},
	]
}
