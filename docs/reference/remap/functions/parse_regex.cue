package metadata

remap: functions: parse_regex: {
	category:    "Parse"
	description: """
		Parses the `value` via the provided [Regex](\(urls.regex)) `pattern`.

		This function differs from the `parse_regex_all` function in that it returns the first match only.
		"""
	notices: [
		"""
		VRL aims to provide purpose-specific [parsing functions](\(urls.vrl_parsing_functions)) for common log formats.
		Before reaching for the `parse_regex` function, see if a Remap [`parse_*` function](\(urls.vrl_parsing_functions))
		already exists for your format. If not, please consider [opening an issue](\(urls.new_feature_request)) to
		request support.
		""",
		"""
			All values are returned as strings, it is recommended to manually coerce values as you see fit.
			""",
	]

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
		types: ["object"]
		rules: [
			"Matches will return the capture groups corresponding to the leftmost matches in the text.",
			"If no match is found an error is raised.",
		]
	}

	examples: [
		{
			title: "Parse via Regex (with capture groups)"
			source: """
				parse_regex("first group and second group.", r'(?P<number>.*?) group')
				"""
			return: {
				number: "first"
				"0":    "first group"
				"1":    "first"
			}
		},
		{
			title: "Parse via Regex (without capture groups)"
			source: """
				parse_regex("first group and second group.", r'(?.*?) group')
				"""
			return: {
				"1": "first"
			}
		},
	]
}
