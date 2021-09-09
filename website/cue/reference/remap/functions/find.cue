package metadata

remap: functions: find: {
	category: "String"
	description: """
		Determines whether the elements in the `value` matches the `pattern` and returns its position or -1.
		"""

	arguments: [
		{
			name:        "value"
			description: "The string."
			required:    true
			type: ["string"]
		},
		{
			name:        "pattern"
			description: "The regular expression or string pattern to match against."
			required:    true
			type: ["regex", "string"]
		},
		{
			name:        "from"
			description: "Offset to start searching."
			required:    false
			default:     0
			type: ["integer"]
		},
	]
	internal_failure_reasons: [
		"Unable to find any matching term."
	]
	return: types: ["integer"]

	examples: [
		{
			title: "Match text"
			source: #"""
					find("foobar", "foo")
				"""#
			return: 0
		},
		{
			title: "Match regex"
			source: #"""
					find("foobar", r'b.r')
				"""#
			return: 3
		},
		{
			title: "With an offset"
			source: #"""
					find("foobarfoobarfoo", "bar", 4)
				"""#
			return: 9
		},
	]
}
