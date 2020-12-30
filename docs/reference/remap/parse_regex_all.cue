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
			title: "Successful match"
			input: {
				message: "first group and second group."
			}
			source: ".result = parse_regex_all(.message, /(?P<number>.*?) group/)"
			output: {
				result: [ {number: "first"
							"0": "first group"
							"1": "first"
				},
					{number: "second"
							"0": "second group"
							"1": "second"
					}]
			}
		},
	]
}
