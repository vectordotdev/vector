package metadata

remap: functions: exists: {
	category: "Path"
	description: """
		Checks whether the `path` exists for the target.

		This function allows you to distinguish between a missing path,
		or a path with a `null` value, something a regular path lookup
		such as `.foo` would not allow, since that always returns `null`
		if the path doesn't exist.
		"""

	arguments: [
		{
			name:        "path"
			description: "The path of the field to check."
			required:    true
			type: ["path"]
		},
	]
	internal_failure_reasons: []
	return: types: ["boolean"]

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
			title: "Exists (array element)"
			input: log: array: [1, 2, 3]
			source: #"""
				exists(.array[2])
				"""#
			return: true
		},
	]
}
