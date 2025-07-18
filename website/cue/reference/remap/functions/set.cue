package metadata

remap: functions: set: {
	category: "Path"
	description: """
		Dynamically insert data into the path of a given object or array.

		If you know the path you want to assign a value to,
		use static path assignments such as `.foo.bar[1] = true` for
		improved performance and readability. However, if you do not
		know the path names, use the dynamic `set` function to
		insert the data into the object or array.
		"""

	arguments: [
		{
			name:        "value"
			description: "The object or array to insert data into."
			required:    true
			type: ["object", "array"]
		},
		{
			name:        "path"
			description: "An array of path segments to insert the value into."
			required:    true
			type: ["array"]
		},
		{
			name:        "data"
			description: "The data to be inserted."
			required:    true
			type: ["any"]
		},
	]
	internal_failure_reasons: [
		#"The `path` segment must be a string or an integer."#,
	]
	return: types: ["object", "array"]

	examples: [
		{
			title: "single-segment top-level field"
			source: #"""
				set!(value: { "foo": "bar" }, path: ["foo"], data: "baz")
				"""#
			return: foo: "baz"
		},
		{
			title: "multi-segment nested field"
			source: #"""
				set!(value: { "foo": { "bar": "baz" } }, path: ["foo", "bar"], data: "qux")
				"""#
			return: foo: bar: "qux"
		},
		{
			title: "array"
			source: #"""
				set!(value: ["foo", "bar", "baz"], path: [-2], data: 42)
				"""#
			return: ["foo", 42, "baz"]
		},
	]
}
