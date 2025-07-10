package metadata

remap: functions: strip_ansi_escape_codes: {
	category:    "String"
	description: """
		Strips [ANSI escape codes](\(urls.ansi_escape_codes)) from `value`.
		"""

	arguments: [
		{
			name:        "value"
			description: "The string to strip."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: []
	return: types: ["string"]

	examples: [
		{
			title: "Strip ANSI escape codes"
			source: #"""
				strip_ansi_escape_codes("\e[46mfoo\e[0m bar")
				"""#
			return: "foo bar"
		},
	]
}
