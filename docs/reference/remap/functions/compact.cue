package metadata

remap: functions: compact: {
	category: "Enumerate"
	description: """
		Compacts the `value` by removing "empty" values.

		What is considered empty can be specified with the parameters.
		"""

	arguments: [
		{
			name:        "value"
			description: "The object or array to compact."
			required:    true
			type: ["array", "object"]
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
			name:        "object"
			description: "Should an empty object be treated as an empty value."
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
	internal_failure_reasons: []
	return: {
		types: ["array", "object"]
		rules: [
			"The return type will match the `value` type.",
		]
	}
	examples: [
		{
			title: "Compact an array"
			source: #"""
				compact(["foo", "bar", "", null, [], "buzz"], string: true, array: true, null: true)
				"""#
			return: ["foo", "bar", "buzz"]
		},
		{
			title: "Compact an object"
			source: #"""
				compact({"field1": 1, "field2": "", "field3": [], "field4": null}, string: true, array: true, null: true)
				"""#
			return: field1: 1
		},
	]
}
