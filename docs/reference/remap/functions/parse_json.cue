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
	internal_failure_reasons: [
		"`value` is not a valid JSON formatted payload",
	]
	return: ["boolean", "integer", "float", "string", "map", "array", "null"]
	category: "Parse"
	description: #"""
		Parses the provided `value` as JSON.

		Only JSON types are returned. If you need to convert a `string` into a `timestamp`, consider the
		`parse_timestamp` function.
		"""#
	examples: [
		{
			title: "Parse JSON"
			source: #"""
				parse_json("{\"key\": \"val\"}")
				"""#
			return: key: "val"
		},
	]
}
