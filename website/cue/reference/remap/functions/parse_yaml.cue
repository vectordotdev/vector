package metadata

remap: functions: parse_yaml: {
	category: "Parse"
	description: """
		Parses the `value` as YAML.
		"""
	notices: [
		"""
			Only YAML types are returned. If you need to convert a `string` into a `timestamp`, consider the
			[`parse_timestamp`](#parse_timestamp) function.
			""",
	]

	arguments: [
		{
			name:        "value"
			description: "The string representation of the YAML to parse."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: [
		"`value` is not a valid YAML-formatted payload.",
	]
	return: types: ["boolean", "integer", "float", "string", "object", "array", "null"]

	examples: [
		{
			title: "Parse YAML"
			source: #"""
				parse_yaml!("key: val")
				"""#
			return: key: "val"
		},
		{
			title: "Parse embedded JSON"
			source: #"""
				parse_yaml!("{\"key\": \"val\"}")
				"""#
			return: key: "val"
		},
	]
}
