package metadata

remap: functions: remove: {
	category: "Path"
	description: """
		Dynamically remove the value for a given path.

		If you know the path you want to remove, use
		the `del` function and static paths such as `del(.foo.bar[1])`
		to remove the value at that path. The `del` function returns the
		deleted value, and is more performant than `remove`.
		However, if you do not know the path names, use the dynamic
		`remove` function to remove the value at the provided path.
		"""

	arguments: [
		{
			name:        "value"
			description: "The object or array to remove data from."
			required:    true
			type: ["object", "array"]
		},
		{
			name:        "path"
			description: "An array of path segments to remove the value from."
			required:    true
			type: ["array"]
		},
		{
			name: "compact"
			description: """
				After deletion, if `compact` is `true`, any empty objects or
				arrays left are also removed.
				"""
			required: false
			default:  false
			type: ["boolean"]
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
				remove!(value: { "foo": "bar" }, path: ["foo"])
				"""#
			return: {}
		},
		{
			title: "multi-segment nested field"
			source: #"""
				remove!(value: { "foo": { "bar": "baz" } }, path: ["foo", "bar"])
				"""#
			return: foo: {}
		},
		{
			title: "array indexing"
			source: #"""
				remove!(value: ["foo", "bar", "baz"], path: [-2])
				"""#
			return: ["foo", "baz"]
		},
		{
			title: "compaction"
			source: #"""
				remove!(value: { "foo": { "bar": [42], "baz": true } }, path: ["foo", "bar", 0], compact: true)
				"""#
			return: foo: baz: true
		},
	]
}
