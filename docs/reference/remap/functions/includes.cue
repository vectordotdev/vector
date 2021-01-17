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
	internal_failure_reasons: []
	return: ["boolean"]
	category: "Enumerate"
	description: """
		Determines whether the provided `values` contains the provided `item`.
		"""
	examples: [
		{
			title: "Array includes"
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
	]
}
