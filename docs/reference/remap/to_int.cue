remap: functions: to_int: {
	arguments: [
		{
			name:        "value"
			description: "The string that is to be converted to an int. Must be the string representation of an `int`, otherwise, an `ArgumentError` will be raised."
			required:    true
			type: ["integer", "float", "boolean", "string"]
		},
	]
	return: ["integer"]
	category: "coerce"
	description: #"""
		Returns an `integer` whose text representation is `string`.
		"""#
	examples: [
		{
			title: "Success"
			input: {
				integer: "2"
			}
			source: ".integer = to_int(.integer)"
			output: {
				integer: 2
			}
		},
		{
			title: "Error"
			input: {
				integer: "hi"
			}
			source: ".integer = to_int(.integer)"
			output: {
				error: remap.errors.ArgumentError
			}
		},
	]
}
