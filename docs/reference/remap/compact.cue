package metadata

remap: functions: compact: {
	arguments: [
		{
			name:        "value"
			description: "The map or array to compact."
			required:    true
			type: ["array", "map"]
		},
		{
			name:        "recursive"
			description: "Should the compact be recursive."
			required:    false
			default:     true
			type: ["boolean"]
		},
		{
			name:        "null"
			description: "Should null be treated as an empty value."
			required:    false
			default:     true
			type: ["boolean"]
		},
		{
			name:        "string"
			description: "Should an empty string be treated as an empty value."
			required:    false
			default:     true
			type: ["boolean"]
		},
		{
			name:        "map"
			description: "Should an empty map be treated as an empty value."
			required:    false
			default:     true
			type: ["boolean"]
		},
		{
			name:        "array"
			description: "Should an empty array be treated as an empty value."
			required:    false
			default:     true
			type: ["boolean"]
		},
		{
			name:        "nullish"
			description: #"Tests if the value is "nullish" as defined by the `is_nullish` function."#
			required:    false
			default:     false
			type: ["boolean"]
		},
	]
	internal_failure_reason: null
	return: ["array", "map"]
	category: "Enumerate"
	description: #"""
		Compacts an `array` or `map` by removing "empty" values.

		What is considered empty can be specified with the parameters.
		"""#
	examples: [
		{
			title: "Compact an array"
			input: log: array: ["foo", "bar", "", null, [], "buzz"]
			source: #"""
				.log = compact(.array, string: true, array: true, null: true)
				"""#
			output: log: array: ["foo", "bar", "buzz"]
		},
		{
			title: "Compact a map"
			input: map: {
				field1: 1
				field2: ""
				field3: []
				field4: null
			}
			source: #"""
				.map = compact(.map, string: true, array: true, null: true)
				"""#
			output: map: field1: 1
		},
	]
}
