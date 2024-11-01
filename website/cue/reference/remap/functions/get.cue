package metadata

remap: functions: get: {
	category: "Path"
	description: """
		Dynamically get the value of a given path.

		If you know the path you want to look up, use
		static paths such as `.foo.bar[1]` to get the value of that
		path. However, if you do not know the path names,
		use the dynamic `get` function to get the requested
		value.
		"""

	arguments: [
		{
			name:        "value"
			description: "The object or array to query."
			required:    true
			type: ["object", "array"]
		},
		{
			name:        "path"
			description: "An array of path segments to look for the value."
			required:    true
			type: ["array"]
		},
	]
	internal_failure_reasons: [
		#"The `path` segment must be a string or an integer."#,
	]
	return: types: ["any"]

	examples: [
		{
			title: "single-segment top-level field"
			source: #"""
				get!(value: { "foo": "bar" }, path: ["foo"])
				"""#
			return: "bar"
		},
		{
			title: "multi-segment nested field"
			source: #"""
				get!(value: { "foo": { "bar": "baz" } }, path: ["foo", "bar"])
				"""#
			return: "baz"
		},
		{
			title: "array indexing"
			source: #"""
				get!(value: ["foo", "bar", "baz"], path: [-2])
				"""#
			return: "bar"
		},
	]
}
