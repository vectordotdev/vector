package metadata

remap: functions: filter: {
	category:    "Enumerate"
	description: """
		Filter elements from a collection.

		This function currently *does not* support recursive iteration.
		If you have a need for recursive iteration using `filter`, then
		[please let us know](\(urls.new_feature_request))!

		The function uses the "function closure syntax" to allow reading
		the key/value or index/value combination for each item in the
		collection.

		The same scoping rules apply to closure blocks as they do for
		regular blocks, meaning, any variable defined in parent scopes
		are accessible, and mutations to those variables are preserved,
		but any new variables instantiated in the closure block are
		unavailable outside of the block.

		Check out the examples below to learn about the closure syntax.
		"""

	arguments: [
		{
			name:        "value"
			description: "The array or object to filter."
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
			title: "Filter elements"
			input: log: {
				tags: ["foo", "bar", "foo", "baz"]
			}
			source: #"""
				filter(array!(.tags)) -> |_index, value| {
				    # keep any elements that aren't equal to "foo"
				    value != "foo"
				}
				"""#
			return: ["bar", "baz"]
		},
	]
}
