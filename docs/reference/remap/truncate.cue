package metadata

remap: functions: truncate: {
	arguments: [
		{
			name:        "value"
			description: "The string to truncate."
			required:    true
			type: ["string"]
		},
		{
			name:        "limit"
			description: "The number of characters to truncate the string after."
			required:    true
			type: ["integer", "float"]
		},
		{
			name:        "ellipsis"
			description: "If true, an ellipsis (...) is appended should the string be truncated."
			required:    true
			type: ["boolean"]
		},
	]
	return: ["string"]
	category: "text"
	description: #"""
		Truncates a string after a given number of characters. If `limit` is larger than the length of the string,
		the string is returned unchanged.

		Optionally, an ellipsis (...) is appended if the string does get appended.
		Note: this does increase the string length by 3, so if you need the result to fit in a certain length, specify
		the limit as that length minus 3.
		"""#
	examples: [
		{
			title: "Success"
			input: {
				text: #"A rather long sentence."#
			}
			source: #"""
				.truncated = truncate(.text, limit = 11, ellipsis = true)
				"""#
			output: {
				text:      #"A rather long sentence."#
				truncated: "A rather lo..."
			}
		},
		{
			title: "Error"
			input: {
				text: 42
			}
			source: #"""
				.truncated = truncate(.text, limit = 11, ellipsis = true)
				"""#
			output: {
				error: remap.errors.ArgumentError
			}
		},
	]
}
