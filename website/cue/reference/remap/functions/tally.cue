package metadata

remap: functions: unique: {
	category: "Enumerate"
	description: #"""
		Return counts of the occurrences of each element in an array.
		"""#

	arguments: [
		{
			name:        "value"
			description: "The array to return counts of elements from."
			required:    true
			type: ["array"]
		},
	]
	internal_failure_reasons: []
	return: {
		types: ["object"]
	}

	examples: [
		{
			title: "Unique"
			source: #"""
				unique(["foo", "bar", "foo", "baz"])
				"""#
			return: {"foo": 2, "bar": 1, "baz": 1}
		},
	]
}
