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
	category: "text"
	description: #"""
		Removes the any ANSI escape codes from the provided string.
		"""#
	examples: [
		{
			title: "Success"
			input: {
				text: #"\e[46mfoo\e[0m bar"#
			}
			source: #"""
				.text = strip_ansi_escape_codes(.text)
				"""#
			output: {
				text: "foo bar"
			}
		},
		{
			title: "Error"
			input: {
				text: 37
			}
			source: #"""
					.text = strip_ansi_escape_codes(.text)
				"""#
			output: {
				error: remap.errors.ArgumentError
			}
		},
	]
}
