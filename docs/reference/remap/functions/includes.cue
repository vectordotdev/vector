package metadata

remap: functions: includes: {
	category: "Enumerate"
	description: """
		Determines whether the `value` includes the `item`.
		"""

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
	return: types: ["boolean"]

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
