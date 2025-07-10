package metadata

remap: functions: parse_csv: {
	category: "Parse"
	description: #"""
		Parses a single CSV formatted row. Only the first row is parsed in case of multiline input value.
		"""#
	notices: [
		"""
			All values are returned as strings. We recommend manually coercing values to desired types as you see fit.
			""",
	]

	arguments: [
		{
			name:        "value"
			description: "The string to parse."
			required:    true
			type: ["string"]
		},
		{
			name:        "delimiter"
			description: "The field delimiter to use when parsing. Must be a single-byte utf8 character."
			required:    false
			default:     ","
			type: ["string"]
		},
	]
	internal_failure_reasons: [
		"The delimiter must be a single-byte UTF-8 character.",
		"`value` is not a valid CSV string.",
	]
	return: types: ["array"]

	examples: [
		{
			title: "Parse a single CSV formatted row"
			source: #"""
				parse_csv!("foo,bar,\"foo \"\", bar\"")
				"""#
			return: ["foo", "bar", #"foo ", bar"#]
		},
		{
			title: "Parse a single CSV formatted row with custom delimiter"
			source: #"""
				parse_csv!("foo bar", delimiter: " ")
				"""#
			return: ["foo", "bar"]
		},
	]
}
