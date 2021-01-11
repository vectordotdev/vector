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
	category: "Parse"
	description: #"""
		Returns an `object` whose text representation is a JSON
		payload in `string` form.

		`string` must be the string representation of a JSON
		payload. Otherwise, an `ParseError` will be raised.
		"""#
	examples: [
		{
			title: "Parse JSON (success)"
			input: log: message: #"{"key": "val"}"#
			source: #"""
				. = parse_json(del(.message))
				"""#
			output: log: key: "val"
		},
		{
			title: "Parse JSON (success)"
			input: log: message: "{\"malformed\":"
			source: ". = parse_json(del(.message))"
			raise:  "Failed to parse"
		},
	]
}
