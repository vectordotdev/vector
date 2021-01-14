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
	internal_failure_reason: null
	return: ["timestamp"]
	category: "Number"
	description: #"""
		Rounds the given `value` down to the specified `precision`.
		"""#
	examples: [
		{
			title: "Floor a number (without precision)"
			input: log: number: 4.345
			source: #"""
				.floor = floor(.number, precision: 2)
				"""#
			output: log: number: 4
		},
		{
			title: "Floor a number (with precision)"
			input: log: number: 4.345
			source: #"""
				.floor = floor(.number, precision: 2)
				"""#
			output: log: number: 4.34
		},
	]
}
