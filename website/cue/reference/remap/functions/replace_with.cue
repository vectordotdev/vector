package metadata

remap: functions: replace_with: {
	category: "String"
	description: """
		Replaces all matching instances of `pattern` using a closure.

		The `pattern` argument accepts a regular expression that can use capture groups.

		The function uses the function closure syntax to compute the replacement values.

		The closure takes a single parameter, which is an array, where the first item is always
		present and contains the entire string that matched `pattern`. The items from index one on
		contain the capture groups of the corresponding index. If a capture group is optional, the
		value may be null if it didn't match.

		The value returned by the closure must be a string and will replace the section of
		the input that was matched.

		This returns a new string with the replacements, the original string is not mutated.
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
			description: "Replace all matches of this pattern. Must be a regular expression."
			required:    true
			type: ["regex"]
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
			title: "Capitalize words"
			source: #"""
					replace_with("apples and bananas", r'\b(\w)(\w*)') -> |match| {
						upcase!(match.captures[0]) + string!(match.captures[1])
					}
				"""#
			return: "Apples And Bananas"
		},
		{
			title: "Replace with hash"
			source: #"""
					replace_with("email from test@example.com", r'\w+@example.com') -> |match| {
						sha2(match.string, variant: "SHA-512/224")
					}
				"""#
			return: "email from adf6e1bc4415d24912bd93072ad34ef825a7b6eb3bf53f68def1fc17"
		},
		{
			title: "Replace first instance"
			source: #"""
					replace_with("Apples and Apples", r'(?i)apples|cones', count: 1) -> |match| {
						"Pine" + downcase(match.string)
					}
				"""#
			return: "Pineapples and Apples"
		},
		{
			title: "Named capture group"
			source: #"""
					replace_with("level=error A message", r'level=(?P<level>\w+)') -> |match| {
						lvl = upcase!(match.level)
						"[{{lvl}}]"
					}
				"""#
			return: "[ERROR] A message"
		},
	]
}
