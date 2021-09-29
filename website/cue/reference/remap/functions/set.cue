package metadata

remap: functions: insert: {
	category: "Path"
	description: """
		Dynamically insert data into the path of a given object or array.

		When you know the path you want to assign a value to, you should
		use static path assignments such as `.foo.bar[1] = true` for
		improved performance and readability. However, when you don't
		know the path names in advance, you can use this dynamic
		insertion function to insert the data into the object or array.
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
			description: "An array of path segments to insert the value to."
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
		#"path segment must be either "string" or "integer""#,
	]
	return: types: ["object", "array"]

	examples: [
		{
			title: "single-segment top-level field"
			source: #"""
				insert!(value: { "foo": "bar" }, path: ["foo"], data: "baz")
				"""#
			return: foo: "baz"
		},
		{
			title: "multi-segment nested field"
			source: #"""
				insert!(value: { "foo": { "bar": "baz" } }, path: ["foo", "bar"], data: "qux")
				"""#
			return: foo: bar: "qux"
		},
		{
			title: "array"
			source: #"""
				insert!(value: ["foo", "bar", "baz"], path: [-2], data: 42)
				"""#
			return: ["foo", 42, "baz"]
		},
	]
}
