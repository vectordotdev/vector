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
	return: ["map"]
	category: "Parse"
	description: """
		Searches within the text for capture groups specified by the provided regular expression. It will
		return the capture groups corresponding to the leftmost matches in the text. If no match is found
		an empty map is returned.
		"""
	examples: [
		{
			title: "Successful match"
			input: {
				message: "first group and second group."
			}
			source: ". = parse_regex(.message, /(?P<number>.*?) group/)"
			output: {
				number: "first"
				"0":    "first group"
				"1":    "first"
			}
		},
	]
}
