package metadata

remap: functions: for_each: {
	category: "Enumerate"
	description: #"""
		Iterate over a collection.
		"""#

	arguments: [
		{
			name:        "value"
			description: "The array or object to iterate."
			required:    true
			type: ["array", "object"]
		},
		{
			name:        "recursive"
			description: "Whether to recursively iterate the collection."
			required:    false
			type: ["boolean"]
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
				.tally = {}
				for_each(array!(.tags)) -> |_index, value| {
				    # Get the current tally for the `value`, or
				    # set to `0`.
				    count = int(get!(.tally, [value])) ?? 0
				
				    # Increment the tally for the value by `1`.
				    .tally = set!(.tally, [value], count + 1)
				}
				
				.tally
				"""#
			return: {"foo": 2, "bar": 1, "baz": 1}
		},
	]
}
