package metadata

remap: functions: parse_ruby_hash: {
	category: "Parse"
	description: """
		Parses the `value` as ruby hash.
		"""

	notices: [
		"""
			Only ruby types are returned. If you need to convert a `string` into a `timestamp`, consider the
			[`parse_timestamp`](#parse_timestamp) function.
			""",
	]
	arguments: [
		{
			name:        "value"
			description: "The string representation of the ruby hash to parse."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: [
		"`value` is not a valid ruby hash formatted payload.",
	]

	return: types: ["object"]

	examples: [
		{
			title: "Parse ruby hash"
			source: """
				parse_ruby_hash!(s'{ "test" => "value", "testNum" => 0.2, "testObj" => { "testBool" => true, "testNull" => nil } }')
				"""
			return: {
				test:    "value"
				testNum: 0.2
				testObj: {
					testBool: true
					testNull: null
				}
			}
		},
	]
}
