package metadata

remap: functions: format_number: {
	arguments: [
		{
			name:        "value"
			description: "The number to format as a string."
			required:    true
			type: ["integer", "float"]
		},
		{
			name:        "scale"
			description: "The number of decimal places to display."
			required:    false
			type: ["integer"]
		},
		{
			name:        "decimal_separator"
			description: "The character to use between the whole and decimal parts of the number."
			required:    false
			type: ["string"]
			default: "."
		},
		{
			name:        "grouping_separator"
			description: "The character to use between each thousands part of the number."
			required:    false
			type: ["string"]
			default: ","
		},
	]
	internal_failure_reasons: []
	return: ["string"]
	category: "Number"
	description: #"""
		Formats the given `value` into a string representation of the number.
		"""#
	examples: [
		{
			title: "Format a number (3 decimals)"
			source: #"""
				format_number(1234567.89, 3, decimal_separator: ".", grouping_separator: ",")
				"""#
			return: "1,234,567.890"
		},
	]
}
