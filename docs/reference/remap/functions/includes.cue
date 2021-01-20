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
			source: #"""
				includes(["apple", "orange", "banana"], "banana")
				"""#
			return: true
		},
	]
}
