package metadata

remap: functions: map_values: {
	category: "Enumerate"
	description: #"""
		Map the values within a collection.

		If `recursive` is enabled, the function iterates into nested
		collections, using the following rules:

		1. Iteration starts at the root.
		2. For every nested collection type:
		   - First return the collection type itself.
		   - Then recurse into the collection, and loop back to item (1)
		     in the list
		   - Any mutation done on a collection *before* recursing into it,
		     are preserved.

		The function uses the function closure syntax to allow mutating
		the value for each item in the collection.

		The same scoping rules apply to closure blocks as they do for
		regular blocks, meaning, any variable defined in parent scopes
		are accessible, and mutations to those variables are preserved,
		but any new variables instantiated in the closure block are
		unavailable outside of the block.

		Check out the examples below to learn about the closure syntax.
		"""#

	arguments: [
		{
			name:        "value"
			description: "The object or array to iterate."
			required:    true
			type: ["array", "object"]
		},
		{
			name:        "recursive"
			description: "Whether to recursively iterate the collection."
			required:    false
			default:     false
			type: ["boolean"]
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
