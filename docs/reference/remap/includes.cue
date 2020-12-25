package metadata

remap: functions: includes: {
	arguments: [
		{
			name:        "value"
			description: "The array"
			required:    true
			type: ["array"]
		},
		{
			name:        "item"
			description: "The item to check"
			required:    true
			type: ["any"]
		},
	]
	return: ["boolean"]
	category: "Enumerable"
	description: """
		Determines whether an item is contained in an array. The item can be of any type and arrays
		can be of mixed types.
		"""
	examples: [
		{
			title: "String array"
			input: {
				fruits: ["apple", "orange", "banana"]
			}
			source: #"""
				.includes_banana = includes(.fruits, "banana")
				.includes_mango = includes(.fruits, "mango")
				"""#
			output: {
				includes_banana: true
				includes_mango:  false
			}
		},
		{
			title: "Mixed array"
			input: {
				kitchen_sink: ["hello", 72.5, false, [1, 2, 3]]
			}
			source: #"""
				.includes_empty_list = includes(.kitchen_sink, [])
				.includes_hello = includes(.kitchen_sink, "hello")
				.includes_false = includes(.kitchen_sink, false)
				"""#
			output: {
				includes_empty_list: false
				includes_hello:      true
				includes_false:      true
			}
		},
	]
}
