package metadata

remap: functions: parse_regex_all: {
	category:    "Parse"
	description: """
		Parses the `value` via the provided [Regex](\(urls.regex)) `pattern`.

		This function differs from the `parse_regex` function in that it returns _all_ matches, not just the first.
		"""
	notices:     remap.functions.parse_regex.notices

	arguments: [
		{
			name:        "value"
			description: "The string to search."
			required:    true
			type: ["string"]
		},
		{
			name:        "pattern"
			description: "The regular expression pattern to search against."
			required:    true
			type: ["regex"]
		},
	]
	internal_failure_reasons: [
		"`value` fails to parse via the provided `pattern`",
	]
	return: {
		types: ["array"]
		rules: [
			"Matches will return all capture groups corresponding to the leftmost matches in the text.",
			"If no match is found an empty map is returned.",
		]
	}

	examples: [
		{
			title: "Parse via Regex (all matches)"
			source: """
				parse_regex_all("first group and second group.", r'(?P<number>.*?) group')
				"""
			return: [
				{
					number: "first"
					"0":    "first group"
					"1":    "first"
				},
				{
					number: "second"
					"0":    "second group"
					"1":    "second"
				},
			]
		},
	]
}
