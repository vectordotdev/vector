package metadata

remap: functions: parse_json: {
	arguments: [
		{
			name:        "value"
			description: "The string representation of the JSON to parse."
			required:    true
			type: ["string"]
		},
	]
	return: ["boolean", "integer", "float", "string", "map", "array", "null"]
	category: "parse"
	description: #"""
		Returns an `object` whose text representation is a JSON
		payload in `string` form.

		`string` must be the string representation of a JSON
		payload. Otherwise, an `ParseError` will be raised.
		"""#
	examples: [
		{
			title: "Success"
			input: {
				message: #"{"key": "val"}"#
			}
			source: #"""
				. = parse_json(.message)
				"""#
			output: {
				key: "val"
			}
		},
		{
			title: "Error"
			input: {
				message: "{\"malformed\":"
			}
			source: "parse_json(.message)"
			output: {
				error: remap.errors.ParseError
			}
		},
	]
}
