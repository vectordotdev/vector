package metadata

remap: functions: round: {
	arguments: [
		{
			name:        "value"
			description: "The number to round."
			required:    true
			type: ["integer", "float"]
		},
		{
			name:        "precision"
			description: "The number of decimal places to round to."
			required:    false
			default:     0
			type: ["integer"]
		},
	]
	return: ["timestamp"]
	category: "numeric"
	description: #"""
		Rounds the given number to the given number of decimal places. Rounds up or down
		depending on which is nearest. Numbers that are half way (5) are rounded up.
		"""#
	examples: [
		{
			title: "Round up"
			input: {
				number: 4.345
			}
			source: #"""
				.floor = floor(.number, precision = 2)
				"""#
			output: {
				number: 4.345
				floor:  4.35
			}
		},
		{
			title: "Round down"
			input: {
				number: 4.344
			}
			source: #"""
				.floor = floor(.number, precision = 2)
				"""#
			output: {
				number: 4.344
				floor:  4.34
			}
		},
	]
}
