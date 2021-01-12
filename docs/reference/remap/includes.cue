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
	internal_failure_reason: null
	return: ["boolean"]
	category: "Enumerate"
	description: """
		Determines whether the provided `values` contains the provided `item`.
		"""
	examples: [
		{
			title: "String array includes"
			input: log: fruits: ["apple", "orange", "banana"]
			source: #"""
				.includes_banana = includes(.fruits, "banana")
				.includes_mango = includes(.fruits, "mango")
				"""#
			output: input & {log: {
				includes_banana: true
				includes_mango:  false
			}}
		},
		{
			title: "Mixed array includes"
			input: log: kitchen_sink: ["hello", 72.5, false, [1, 2, 3]]
			source: #"""
				.includes_empty_list = includes(.kitchen_sink, [])
				.includes_hello = includes(.kitchen_sink, "hello")
				.includes_false = includes(.kitchen_sink, false)
				"""#
			output: input & {log: {
				includes_empty_list: false
				includes_hello:      true
				includes_false:      true
			}}
		},
	]
}
