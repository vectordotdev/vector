package metadata

remap: functions: type_of: {
	category: "Type"
	description: """
		Returns the type of a `value`.

		The possible VRL types are: `array`, `boolean`, `bytes`, `float`, `integer`, `null`,
		`object`, `regex` and `timestamp`.
		"""

	arguments: [
		{
			name:        "value"
			description: #"The value to get the type for"#
			required:    true
			type: ["any"]
		},
	]
	internal_failure_reasons: []
	return: types: ["string"]

	examples: [
		{
			title: "Get the type of a bytes sequence (string)"
			source: """
				type_of("a string")
				"""
			return: "bytes"
		},
		{
			title: "Get the type for a null value"
			source: """
				type_of(null)
				"""
			return: "null"
		},
		{
			title: "Get the type for an array"
			source: """
				type_of([1, 2, 3])
				"""
			return: "array"
		},
		{
			title: "Get the type for an object"
			source: """
				type_of({"foo": "bar"})
				"""
			return: "object"
		},
		{
			title: "Get the type for a regex"
			source: """
				type_of(r'pattern')
				"""
			return: "object"
		},
	]
}
