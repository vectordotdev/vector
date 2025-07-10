package metadata

remap: functions: find: {
	category: "String"
	description: """
		Determines from left to right the start position of the first found element in `value`
		that matches `pattern`. Returns `-1` if not found.
		"""

	arguments: [
		{
			name:        "value"
			description: "The string to find the pattern in."
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
	internal_failure_reasons: []
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
			title: "No matches"
			source: #"""
				find("foobar", "baz")
				"""#
			return: -1
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
