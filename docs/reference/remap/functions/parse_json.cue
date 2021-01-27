package metadata

remap: functions: parse_json: {
	category: "Parse"
	description: """
		Parses the `value` as JSON.
		"""
	notices: [
		"""
			Only JSON types are returned. If you need to convert a `string` into a `timestamp`, consider the
			`parse_timestamp` function.
			""",
	]

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
	return: types: ["boolean", "integer", "float", "string", "map", "array", "null"]

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
