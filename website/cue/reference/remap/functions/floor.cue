package metadata

remap: functions: floor: {
	category: "Number"
	description: #"""
		Rounds the `value` down to the specified `precision`.
		"""#

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
	return: {
		types: ["integer", "float"]
		rules: [
			"Returns an integer if `precision` is `0` (this is the default). Returns a float otherwise.",
		]
	}

	examples: [
		{
			title: "Round a number down (without precision)"
			source: #"""
				floor(4.345)
				"""#
			return: 4.0
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
