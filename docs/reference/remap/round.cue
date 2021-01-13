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
	internal_failure_reasons: []
	return: ["timestamp"]
	category: "Number"
	description: #"""
		Rounds the provided `value` to number to the specified `precision`.
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
				.number = floor(.number, precision: 2)
				"""#
			output: log: number: 4.35
		},
	]
}
