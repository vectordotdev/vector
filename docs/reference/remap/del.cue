package metadata

remap: functions: del: {
	arguments: [
		{
			name:        "paths"
			description: "The paths of the fields to delete."
			required:    true
			multiple:    true
			type: ["string"]
		},
	]
	return: ["null"]
	category: "event"
	description: #"""
		Removed the fields specified by the given paths from the root `event` object. Multiple fields can be specified.
		"""#
	examples: [
		{
			title: "Success"
			input: {
				"field1": 1
				"field2": 2
				"field3": 3
			}
			source: #"""
				del(.field1, .field3)
				"""#
			output: {
				"field2": 2
			}
		},
	]
}
