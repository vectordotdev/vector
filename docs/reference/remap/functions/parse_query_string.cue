package metadata

remap: functions: parse_query_string: {
	category: "Parse"
	description: #"""
		Parses the `value` as a query string.
		"""#
	notices: [
		"""
			All values are returned as strings. We recommend manually coercing values to desired types as you see fit. Empty keys and values are allowed.
			""",
	]

	arguments: [
		{
			name:        "value"
			description: "The string to parse."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: []
	return: types: ["object"]

	examples: [
		{
			title: "Parse query string"
			source: #"""
				parse_query_string("foo=%2B1&bar=2&bar=3&xyz")
				"""#
			return: {
				foo: "+1"
				bar: ["2", "3"]
				xyz: ""
			}
		},
		{
			title: "Parse Ruby on Rails' query string"
			source: #"""
				parse_query_string("?foo%5b%5d=1&foo%5b%5d=2")
				"""#
			return: {
				"foo[]": ["1", "2"]
			}
		},
	]
}
