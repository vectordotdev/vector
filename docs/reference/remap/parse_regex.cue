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
	examples: [
		{
			title: "Parse via Regex (with capture groups)"
			input: log: message: "first group and second group."
			source: ". = parse_regex(del(.message), /(?P<number>.*?) group/)"
			output: log: {
				number: "first"
				"0":    "first group"
				"1":    "first"
			}
		},
		{
			title: "Parse via Regex (without capture groups)"
			input: log: message: "first group and second group."
			source: ". = parse_regex(del(.message), /(?.*?) group/)"
			output: log: {
				"1": "first"
			}
		},
	]
}
