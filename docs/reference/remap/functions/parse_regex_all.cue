package metadata

remap: functions: parse_regex_all: {
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
	category: "Parse"
	description: """
		Parses the provided `value` via the provided Regex `pattern`.
		"""
	examples: [
		{
			title: "Parse via Regex (all matches)"
			source: """
				parse_regex_all("first group and second group.", /(?P<number>.*?) group/)
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
