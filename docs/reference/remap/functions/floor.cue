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
	internal_failure_reasons: []
	return: ["timestamp"]
	category: "Number"
	description: #"""
		Rounds the given `value` down to the specified `precision`.
		"""#
	examples: [
		{
			title: "Round a number down (without precision)"
			source: #"""
				floor(4.345)
				"""#
			return: 4
		},
		{
			title: "Round a number down (with precision)"
			source: #"""
				floor(4.345, precision: 2)
				"""#
			return: 4.34
		},
	]
}
