package metadata

remap: functions: del: {
	category: "Event"
	description: """
		Removes the field specified by the `path` from the current event object.
		"""

	arguments: [
		{
			name:        "path"
			description: "The path of the field to delete."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: []
	notices: [
		"""
			The `del` function _modifies the current event in-place_ and returns the value of the deleted field.
			""",
	]
	return: {
		types: ["any"]
		rules: [
			"The return is the value of the field being deleted. If the field does not exist, `null` is returned.",
		]
	}

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
