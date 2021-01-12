remap: functions: to_float: {
	arguments: [
		{
			name:        "value"
			description: "The string that is to be converted to a float. Must be the string representation of a `float`, otherwise an `ArgumentError` will be raised."
			required:    true
			type: ["float", "integer", "boolean", "string"]
		},
	]
	internal_failure_reasons: [
		"`value` is not a supported float representation",
	]
	return: ["float"]
	category: "Coerce"
	description: #"""
		Coerces the provided `values` into a `float`.
		"""#
	examples: [
		{
			title: "Convert string to float"
			input: log: float: "3.14"
			source: ".float = to_float(.float)"
			output: log: float: 3.14
		},
	]
}
