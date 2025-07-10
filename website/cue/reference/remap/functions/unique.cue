package metadata

remap: functions: unique: {
	category: "Enumerate"
	description: #"""
		Returns the unique values for an array.

		The first occurrence of each element is kept.
		"""#

	arguments: [
		{
			name:        "value"
			description: "The array to return unique elements from."
			required:    true
			type: ["array"]
		},
	]
	internal_failure_reasons: []
	return: {
		types: ["array"]
	}

	examples: [
		{
			title: "Unique"
			source: #"""
				unique(["foo", "bar", "foo", "baz"])
				"""#
			return: ["foo", "bar", "baz"]
		},
	]
}
