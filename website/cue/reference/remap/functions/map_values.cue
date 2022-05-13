package metadata

remap: functions: map_keys: {
	category: "Enumerate"
	description: #"""
		Map the keys within an object.
		"""#

	arguments: [
		{
			name:        "value"
			description: "The object to iterate."
			required:    true
			type: ["object"]
		},
	]
	internal_failure_reasons: []
	return: {
		types: ["object"]
	}
	examples: [
		{
			title: "Upcase keys"
			input: log: {
				foo: "foo"
				bar: "bar"
			}
			source: #"""
				map_keys(.) -> |key| { upcase(key) }
				"""#
			return: {"FOO": "foo", "BAR": "bar"}
		},
	]
}
