package metadata

remap: functions: replace: {
	category: "String"
	description: """
		Replaces all matching instances of `pattern` in `value`.

		The `pattern` argument accepts regular expression capture groups.

		**Note when using capture groups**:
		- You will need to escape the `$` by using `$$` to avoid Vector interpreting it as an
		  [environment variable when loading configuration](/docs/reference/configuration/#escaping)
		- If you want a literal `$` in the replacement pattern, you will also need to escape this
		  with `$$`. When combined with environment variable interpolation in config files this
		  means you will need to use `$$$$` to have a literal `$` in the replacement pattern.
		"""

	arguments: [
		{
			name:        "value"
			description: "The original string."
			required:    true
			type: ["string"]
		},
		{
			name:        "pattern"
			description: "Replace all matches of this pattern. Can be a static string or a regular expression."
			required:    true
			type: ["regex", "string"]
		},
		{
			name:        "with"
			description: "The string that the matches are replaced with."
			required:    true
			type: ["string"]
		},
		{
			name:        "count"
			description: "The maximum number of replacements to perform. `-1` means replace all matches."
			required:    false
			default:     -1
			type: ["integer"]

		},
	]
	internal_failure_reasons: []
	return: types: ["string"]

	examples: [
		{
			title: "Replace literal text"
			source: #"""
				replace("Apples and Bananas", "and", "not")
				"""#
			return: "Apples not Bananas"
		},
		{
			title: "Replace using regular expression"
			source: #"""
				replace("Apples and Bananas", r'(?i)bananas', "Pineapples")
				"""#
			return: "Apples and Pineapples"
		},
		{
			title: "Replace first instance"
			source: #"""
				replace("Bananas and Bananas", "Bananas", "Pineapples", count: 1)
				"""#
			return: "Pineapples and Bananas"
		},
		{
			title: "Replace with capture groups (Note: Use `$$num` in config files)"
			source: #"""
				replace("foo123bar", r'foo(?P<num>\d+)bar', "$num")
				"""#
			return: "123"
		},
	]
}
