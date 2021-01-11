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
		Removes the field specified by the given path from the event object. If the field exists,
		the field's value is returned by the delete operation, while `null` is returned if the field
		doesn't exist.
		"""#
	examples: [
		{
			title: "Success"
			input: {
				"field1": 1
				"field2": 2
			}
			source: "del(.field1)"
			output: {
				"field2": 2
			}
		},
		{
			title: "Delete existing only"
			input: {
				"user_id": 1
			}
			source: #"""
				.user.id = if exists(.user_id) {
					del(.user_id)
				} else {
					"unknown"
				}
				"""#
			output: {
				user: {
					"id": 1
				}
			}
		},
		{
			title: "Rename field"
			input: {
				old_field: "please rename me"
			}
			source: ".new_field = del(.old_field)"
			output: {
				new_field: "please rename me"
			}
		},
	]
}
