package metadata

remap: functions: parse_regex_all: {
	category:    "Parse"
	description: """
		Parses the `value` using the provided [Regex](\(urls.regex)) `pattern`.

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
		{
			name: "numeric_groups"
			description: """
				If `true`, the index of each group in the regular expression is also captured. Index `0`
				contains the whole match.
				"""
			required: false
			default:  false
			type: ["regex"]
		},
	]
	internal_failure_reasons: [
		"`value` fails to parse using the provided `pattern`.",
	]
	return: {
		types: ["array"]
		rules: [
			"Matches return all capture groups corresponding to the leftmost matches in the text.",
			"Raises an error if no match is found.",
		]
	}

	examples: [
		{
			title: "Parse using Regex (all matches)"
			source: """
				parse_regex_all!("first group and second group.", r'(?P<number>\\w+) group', numeric_groups: true)
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
