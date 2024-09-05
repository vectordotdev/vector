package metadata

remap: functions: parse_json: {
	category: "Parse"
	description: """
		Parses the `value` as JSON.
		"""
	notices: [
		"""
			Only JSON types are returned. If you need to convert a `string` into a `timestamp`, consider the
			[`parse_timestamp`](#parse_timestamp) function.
			""",
	]

	arguments: [
		{
			name:        "value"
			description: "The string representation of the JSON to parse."
			required:    true
			type: ["string"]
		},
		{
			name: "max_depth"
			description: """
				Number of layers to parse for nested JSON-formatted documents.
				The value must be in the range of 1 to 128.
				"""
			required: false
			type: ["integer"]
		},
		{
			name: "lossy"
			description: """
				Whether to parse the JSON in a lossy manner. Replaces invalid UTF-8 characters
				with the Unicode character `�` (U+FFFD) if set to true, otherwise returns an error
				if there are any invalid UTF-8 characters present.
				"""
			required: false
			default:  true
			type: ["boolean"]
		},
	]
	internal_failure_reasons: [
		"`value` is not a valid JSON-formatted payload.",
	]
	return: types: ["boolean", "integer", "float", "string", "object", "array", "null"]

	examples: [
		{
			title: "Parse JSON"
			source: #"""
				parse_json!("{\"key\": \"val\"}")
				"""#
			return: key: "val"
		},
		{
			title: "Parse JSON with max_depth"
			source: #"""
				parse_json!("{\"top_level\":{\"key\": \"val\"}}", max_depth: 1)
				"""#
			return: top_level: "{\"key\": \"val\"}"
		},
	]
}
