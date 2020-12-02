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
	category: "numeric"
	description: #"""
		Rounds the given number up to the given precision of decimal places.
		"""#
	examples: [
		{
			title: "Success"
			input: {
				number: 4.345
			}
			source: #"""
				.ceil = ceil(.number, precision = 2)
				"""#
			output: {
				number: 4.345
				ceil:   4.35
			}
		},
	]
}
