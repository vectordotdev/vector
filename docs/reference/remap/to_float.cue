remap: functions: to_float: {
	arguments: [
		{
			name:        "value"
			description: "The string that is to be converted to a float. Must be the string representation of a `float`, otherwise an `ArgumentError` will be raised."
			required:    true
			type: ["float", "integer", "boolean", "string"]
		},
	]
	return: ["float"]
	category: "coerce"
	description: #"""
		Returns a `float` whose text representation is `string`.
		"""#
	examples: [
		{
			title: "Success"
			input: {
				float: "3.14"
			}
			source: ".float = to_float(.float)"
			output: {
				float: 3.14
			}
		},
		{
			title: "Error"
			input: {
				integer: "hi"
			}
			source: ".float = to_float(.float)"
			output: {
				error: remap.errors.ArgumentError
			}
		},
	]
}
