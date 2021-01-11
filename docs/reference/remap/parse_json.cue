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
			input: log: "message": #"{"key": "val"}"#
			source: #"""
				. = parse_json(del(.message))
				"""#
			output: log: {
				"message": "action:\"Accept\"; flags:\"802832\"; ifdir:\"inbound\"; ifname:\"eth2-05\"; logid:\"6\"; loguid:\"{0x5f0fa4d6,0x1,0x696ac072,0xc28d839a}\";"
			}
		},
	]
}
