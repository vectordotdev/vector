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
	internal_failure_reasons: []
	return: ["any"]
	category: "Event"
	description: #"""
		Removes the field specified by the given path from the event object.

		If the field exists, the field's value is returned by the delete operation, while `null` is returned if the
		field doesn't exist.
		"""#
	examples: [
		{
			title: "Delete a field"
			input: log: {
				field1: 1
				field2: 2
			}
			source: "del(.field1)"
			output: log: field2: 2
		},
		{
			title: "Rename a field"
			input: log: old_field: "please rename me"
			source: ".new_field = del(.old_field)"
			output: log: new_field: "please rename me"
		},
	]
}
