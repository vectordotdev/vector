package metadata

remap: functions: exists: {
	category: "Path"
	description: """
		Checks whether the `path` exists for the target.

		This function distinguishes between a missing path
		and a path with a `null` value. A regular path lookup,
		such as `.foo`, cannot distinguish between the two cases
		since it always returns `null` if the path doesn't exist.
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
