package metadata

remap: functions: object_from_array: {
	category: "Object"
	description: """
		Iterate over either one array of arrays or a pair of arrays and create an object out of all the key-value pairs contained in them.
		With one array of arrays, any entries with no value use `null` instead.
		Any keys that are `null` skip the  corresponding value.

		If a single parameter is given, it must contain an array of all the input arrays.
		"""

	arguments: [
		{
			name:        "values"
			description: "The first array of elements, or the array of input arrays if no other parameter is present."
			required:    true
			type: ["array"]
		},
		{
			name:        "keys"
			description: "The second array of elements. If not present, the first parameter must contain all the arrays."
			required:    false
			type: ["array"]
		},
	]
	internal_failure_reasons: [
		"`values` and `keys` must be arrays.",
		"If `keys` is not present, `values` must contain only arrays.",
	]
	return: {
		types: ["object"]
		rules: [
			"`object_from_array` is considered fallible in the following cases: if any of the parameters is not an array; if only the `value` parameter is present and it is not an array of arrays; or if any of the keys are not either a string or `null`.",
		]
	}

	examples: [
		{
			title: "Create an object from one array"
			source: #"""
				object_from_array([["one", 1], [null, 2], ["two", 3]])
				"""#
			return: {"one": 1, "two": 3}
		},
		{
			title: "Create an object from separate key and value arrays"
			source: #"""
				object_from_array([1, 2, 3], keys: ["one", null, "two"])
				"""#
			return: {"one": 1, "two": 3}
		},
	]
}
