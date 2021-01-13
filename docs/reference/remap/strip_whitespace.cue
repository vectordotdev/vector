package metadata

remap: functions: strip_whitespace: {
	arguments: [
		{
			name:        "value"
			description: "The string to trim."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: []
	return: ["string"]
	category: "String"
	description: #"""
		Strips the whitespace from the start and end of the provided `value`.

		Whitespace is as defined by [Unicode `White_Space` property](\(urls.unicode_whitespace))
		"""#
	examples: [
		{
			title: "Strip whitespace"
			input: log: text: "  A sentence.  "
			source: #"""
				.text = strip_whitespace(.text)
				"""#
			output: log: text: "A sentence."
		},
	]
}
