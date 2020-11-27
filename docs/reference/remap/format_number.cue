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
	return: ["string"]
	category: "coerce"
	description: #"""
		Returns a string representation of the given number.
		"""#
	examples: [
		{
			title: "Success"
			input: {
				number: 1234567.89
			}
			source: #"""
				.formatted = format_number(.number, 3, decimal_separator=".", grouping_separator=",")
				"""#
			output: {
				number:    1234567.89
				formatter: "1,234,567.890"
			}
		},
		{
			title: "Error"
			input: {
				message: "A string with 42"
			}
			source: ".formatted = format_number(.number)"
			output: {
				error: remap.errors.ArgumentError
			}
		},
	]
}
