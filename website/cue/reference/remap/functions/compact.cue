package metadata

remap: functions: compact: {
	category: "Enumerate"
	description: """
		Compacts the `value` by removing empty values, where empty values are defined using the
		available parameters.
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
			description: "Whether the compaction be recursive."
			required:    false
			default:     true
			type: ["boolean"]
		},
		{
			name:        "null"
			description: "Whether null should be treated as an empty value."
			required:    false
			default:     true
			type: ["boolean"]
		},
		{
			name:        "string"
			description: "Whether an empty string should be treated as an empty value."
			required:    false
			default:     true
			type: ["boolean"]
		},
		{
			name:        "object"
			description: "Whether an empty object should be treated as an empty value."
			required:    false
			default:     true
			type: ["boolean"]
		},
		{
			name:        "array"
			description: "Whether an empty array should be treated as an empty value."
			required:    false
			default:     true
			type: ["boolean"]
		},
		{
			name:        "nullish"
			description: #"Tests whether the value is "nullish" as defined by the [`is_nullish`](#is_nullish) function."#
			required:    false
			default:     false
			type: ["boolean"]
		},
	]
	internal_failure_reasons: []
	return: {
		types: ["array", "object"]
		rules: [
			"The return type matches the `value` type.",
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
