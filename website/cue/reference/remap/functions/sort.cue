package metadata

remap: functions: sort: {
	category: "Array"
	description: """
		Sort elements within an array.
		"""

	notices: [
		"""
			Sorting elements of different types is stable, but unspecified. Meaning, currently a string is sorted before an integer, but this ordering might change in future versions.
			""",
	]

	arguments: [
		{
			name:        "value"
			description: "The initial array"
			required:    true
			type: ["array"]
		},
		{
			name:        "reverse"
			description: "If true, the array will be sorted in reverse order"
			required:    false
			type: ["boolean"]
		},
	]
	internal_failure_reasons: []
	return: types: ["array"]

	examples: [
		{
			title: "sort an array"
			source: """
				append([2, 1, 3])
				"""
			return: [1, 2, 3]
		},
		{
			title: "reverse sort an array"
			source: """
				append([2, 1, 3])
				"""
			return: [3, 2, 1]
		},
		{
			title: "sort mixed array"
			source: """
				append([2, null, "foo"])
				"""
			return: ["foo", 2, null]
		},
	]
}
