package metadata

remap: functions: for_each: {
	category:    "Enumerate"
	description: """
		Iterate over a collection.

		This function currently *does not* support recursive iteration.
		If you have a need for recursive iteration using `for_each`,
		which can't be solved using the `map_keys` or `map_values`
		enumeration functions (which *do* support recursion), then
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
			description: "The array or object to iterate."
			required:    true
			type: ["array", "object"]
		},
	]
	internal_failure_reasons: []
	return: {
		types: ["null"]
	}
	examples: [
		{
			title: "Tally elements"
			input: log: {
				tags: ["foo", "bar", "foo", "baz"]
			}
			source: #"""
				tally = {}
				for_each(array!(.tags)) -> |_index, value| {
				    # Get the current tally for the `value`, or
				    # set to `0`.
				    count = int(get!(tally, [value])) ?? 0
				
				    # Increment the tally for the value by `1`.
				    tally = set!(tally, [value], count + 1)
				}
				
				tally
				"""#
			return: {"foo": 2, "bar": 1, "baz": 1}
		},
	]
}
