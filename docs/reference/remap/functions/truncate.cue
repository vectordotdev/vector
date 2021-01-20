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
	internal_failure_reasons: []
	return: ["string"]
	category: "String"
	description: #"""
		Truncates the provided `value` up to the provided `limit`.

		* If `limit` is larger than the length of the string, the string is returned unchanged.
		* If `ellipsis` is `true`, then an ellipsis (...) will be appended to the string (beyond the specified limit).
		"""#
	examples: [
		{
			title: "Truncate a string"
			source: #"""
				truncate("A rather long sentence.", limit = 11, ellipsis = true)
				"""#
			return: "A rather lo..."
		},
	]
}
