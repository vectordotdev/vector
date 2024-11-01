package metadata

remap: functions: format_int: {
	category: "Number"
	description: #"""
		Formats the integer `value` into a string representation using the given base/radix.
		"""#

	arguments: [
		{
			name:        "value"
			description: "The number to format."
			required:    true
			type: ["integer"]
		},
		{
			name:        "base"
			description: "The base to format the number in. Must be between 2 and 36 (inclusive)."
			required:    false
			type: ["integer"]
			default: 10
		},
	]
	internal_failure_reasons: [
		"The base is not between 2 and 36.",
	]
	return: types: ["string"]

	examples: [
		{
			title: "Format as a hexadecimal integer"
			source: #"""
				format_int!(42, 16)
				"""#
			return: "2a"
		},
		{
			title: "Format as a negative hexadecimal integer"
			source: #"""
				format_int!(-42, 16)
				"""#
			return: "-2a"
		},
	]
}
