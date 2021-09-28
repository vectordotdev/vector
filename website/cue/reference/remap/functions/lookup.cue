package metadata

remap: functions: lookup: {
	category: "Path"
	description: """
		Dynamically lookup the value of a given path.

		When you know the path you want to look up, you should use
		static paths such as `.foo.bar[1]` to get the value of that
		path. However, when you don't know the path names in advance,
		you can use this dynamic lookup function to get at the requested
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
			description: "An array of path segments to look up the value for."
			required:    true
			type: ["array"]
		},
	]
	internal_failure_reasons: [
		#"path segment must be either "string" or "integer""#,
	]
	return: types: ["any"]

	examples: [
		{
			title: "single-segment top-level field"
			source: #"""
				lookup!(value: { "foo": "bar" }, path: ["foo"])
				"""#
			return: "bar"
		},
		{
			title: "multi-segment nested field"
			source: #"""
				lookup!(value: { "foo": { "bar": "baz" } }, path: ["foo", "bar"])
				"""#
			return: "baz"
		},
		{
			title: "array indexing"
			source: #"""
				lookup!(value: ["foo", "bar", "baz"], path: [-2])
				"""#
			return: "bar"
		},
	]
}
