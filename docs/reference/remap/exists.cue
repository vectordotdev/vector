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
	return: ["boolean"]
	category: "event"
	description: #"""
		Checks if the given path exists. Nested paths and arrays can also be checked.
		"""#
	examples: [
		{
			title: "Success"
			input: {
				field: 1
			}
			source: #"""
				.exists = exists(.field)
				.doesntexist = exists(.field2)
				"""#
			output: {
				exists:      true
				doesntexist: false
			}
		},
		{
			title: "Arrays"
			input: {
				array: [1, 2, 3]
			}
			source: #"""
				.exists = exists(.array[2])
				.doesntexist = exists(.array[3])
				"""#
			output: {
				exists:      true
				doesntexist: false
			}
		},
	]
}
