package metadata

remap: functions: del: {
	arguments: [
		{
			name:        "path"
			description: "The path of the field to delete."
			required:    true
			type: ["string"]
		},
	]
	return: ["any"]
	category: "Event"
	description: #"""
		Removed the field specified by the given path from the event object. If the field exists,
		the field's value is returned by the delete operation; if the field does not exist, `null`
		is returned.
		"""#
	examples: [
		{
			title: "Success"
			input: {
				"field1": 1
				"field2": 2
			}
			source: #"""
				del(.field1)
				"""#
			output: {
				"field2": 2
			}
		},
	]
}
