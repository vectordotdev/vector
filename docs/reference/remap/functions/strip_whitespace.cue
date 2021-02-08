package metadata

remap: functions: strip_whitespace: {
	category:    "String"
	description: """
		Strips whitespace from the start and end of the `value`.

		Whitespace is as defined by [Unicode `White_Space` property](\(urls.unicode_whitespace))
		"""

	arguments: [
		{
			name:        "value"
			description: "The string to trim."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: []
	return: types: ["string"]

	examples: [
		{
			title: "Strip whitespace"
			source: #"""
				strip_whitespace("  A sentence.  ")
				"""#
			return: "A sentence."
		},
	]
}
