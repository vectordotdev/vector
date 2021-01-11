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
	category: "Number"
	description: #"""
		Rounds the given number to the given number of decimal places. Rounds up or down
		depending on which is nearest. Numbers that are half way (5) are rounded up.
		"""#
	examples: [
		{
			title: "Round (without precision)"
			input: log: number: 4.345
			source: #"""
				.number = floor(.number)
				"""#
			output: log: number: 4
		},
		{
			title: "Round (with precision)"
			input: log: number: 4.345
			source: #"""
				.number = floor(.number, precision = 2)
				"""#
			output: log: number: 4.35
		},
	]
}
