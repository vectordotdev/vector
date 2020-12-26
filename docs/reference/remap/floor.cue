package metadata

remap: functions: floor: {
	arguments: [
		{
			name:        "value"
			description: "The number to round down."
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
	category: "Numeric"
	description: #"""
		Rounds the given number down to the given precision of decimal places.
		"""#
	examples: [
		{
			title: "Success"
			input: {
				number: 4.345
			}
			source: #"""
				.floor = floor(.number, precision = 2)
				"""#
			output: {
				number: 4.345
				floor:  4.34
			}
		},
	]
}
