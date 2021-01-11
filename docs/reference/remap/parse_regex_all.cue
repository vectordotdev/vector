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
	return: ["map"]
	category: "Parse"
	description: """
		Searches within the text for capture groups specified by the provided regular expression. It will
		return an array the capture groups corresponding to all matches in the text. If no match is found
		an empty array is returned.
		"""
	examples: [
		{
			title: "Parse via Regex (all matches)"
			input: log: message: "first group and second group."
			source: ".matches = parse_regex_all(del(.message), /(?P<number>.*?) group/)"
			output: input & {log: matches: [
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
			]}
		},
	]
}
