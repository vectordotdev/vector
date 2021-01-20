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
	internal_failure_reasons: []
	return: ["timestamp"]
	category: "Number"
	description: #"""
		Rounds the given number up to the specified `precision`.
		"""#
	examples: [
		{
			title: "Round a number up (without precision)"
			source: #"""
				ceil(4.345)
				"""#
			return: 4
		},
		{
			title: "Round a number up (with precision)"
			source: #"""
				ceil(4.345, precision: 2)
				"""#
			return: 4.35
		},
	]
}
