package metadata

remap: functions: map_values: {
	category: "Enumerate"
	description: #"""
		Map the values within a collection.
		"""#

	arguments: [
		{
			name:        "value"
			description: "The array or object to iterate."
			required:    true
			type: ["array", "object"]
		},
	]
	internal_failure_reasons: []
	return: {
		types: ["array", "object"]
	}
	examples: [
		{
			title: "Upcase values"
			input: log: {
				foo: "foo"
				bar: "bar"
			}
			source: #"""
				map_values(.) -> |value| { upcase!(value) }
				"""#
			return: {"foo": "FOO", "bar": "BAR"}
		},
	]
}
