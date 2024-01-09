package metadata

remap: functions: del: {
	category: "Path"
	description: """
		Removes the field specified by the static `path` from the target.

		For dynamic path deletion, see the `remove` function.
		"""

	pure: false

	arguments: [
		{
			name:        "path"
			description: "The path of the field to delete."
			required:    true
			type: ["path"]
		},
		{
			name: "compact"
			description: """
				After deletion, if `compact` is `true` and there is an empty object or array left,
				the empty object or array is also removed, cascading up to the root. This only
				applies to the path being deleted, and any parent paths.
				"""
			required: false
			default:  false
			type: ["boolean"]
		},
	]
	internal_failure_reasons: []
	notices: [
		"""
			The `del` function _modifies the current event in place_ and returns the value of the deleted field.
			""",
	]
	return: {
		types: ["any"]
		rules: [
			"Returns the value of the field being deleted. Returns `null` if the field doesn't exist.",
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
