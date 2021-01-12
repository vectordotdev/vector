package metadata

remap: functions: strip_ansi_escape_codes: {
	arguments: [
		{
			name:        "value"
			description: "The string to strip."
			required:    true
			type: ["string"]
		},
	]
	return: ["string"]
	category: "String"
	description: #"""
		Removes the any ANSI escape codes from the provided string.
		"""#
	examples: [
		{
			title: "Strip ANSI escape codes"
			input: log: text: #"\e[46mfoo\e[0m bar"#
			source: #"""
				.text = strip_ansi_escape_codes(.text)
				"""#
			output: log: text: "foo bar"
		},
	]
}
