package metadata

remap: functions: parse_regex: {
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
	return: ["map"]
	category: "Parse"
	description: """
		Parses the provided `value` via the provided Regex `pattern`.

		* Capture groups are supported.
		* Matches will return the capture groups corresponding to the leftmost matches in the text.
		* If no match is found an empty map is returned.
		"""
	notices: [
		"""
		VRL aims to provide purpose-specific [parsing functions](\(urls.vrl_parsing_functions)) for common log formats.
		Before reaching for the `parse_regex` function, see if a Remap [`parse_*` function](\(urls.vrl_parsing_functions))
		already exists for your format. If not, please consider [opening an issue](\(urls.new_feature_request)) to
		request support.
		""",
	]
	examples: [
		{
			title: "Parse via Regex (with capture groups)"
			source: """
				parse_regex("first group and second group.", /(?P<number>.*?) group/)
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
				parse_regex("first group and second group.", /(?.*?) group/)
				"""
			return: {
				"1": "first"
			}
		},
	]
}
