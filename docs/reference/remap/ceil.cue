package metadata

remap: functions: ceil: {
	arguments: [
		{
			name:        "value"
			description: "The number to round up."
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
		Rounds the given number up to the given precision of decimal places.
		"""#
	examples: [
		{
			title: "Ceil a number (without precision)"
			input: log: number: 4.345
			source: #"""
				.number = ceil(.number)
				"""#
			output: log: number: 4
		},
		{
			title: "Ceil a number (with precision)"
			input: log: number: 4.345
			source: #"""
				.number = ceil(.number, precision: 2)
				"""#
			output: log: number: 4.35
		},
	]
}
