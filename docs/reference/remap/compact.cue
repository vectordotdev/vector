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
			description: "Tests if the value is nullish."
			required:    false
			default:     false
			type: ["boolean"]
		},
	]
	return: ["array", "map"]
	category: "Enumerate"
	description: #"""
		Compacts an `Array` or `Map` by removing empty values. What is considered an
		empty value can be specified with the parameters, `null`, `string`, `map`, and
		`array`.
		Specify recursive, if recursive structures should also be compacted, the routine
		will recurse along and `Array`s or `Map`s and compact those structures.
		The following are considered nullish in VRL:

		* `null`
		* A single dash (`"-"`)
		* Any string that contains only whitespace characters as defined by the the [Unicode
		  definition of the `White_Space` property](\(urls.unicode_whitespace)). That includes
		  empty strings (`""`), common characters like `"\n"`, "\r", and others.
		"""#
	examples: [
		{
			title: "Compact an array"
			input: log: array: ["foo", "bar", "", null, [], "buzz"]
			source: #"""
				.log = compact(.array, string = true, array = true, null = true)
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
				.map = compact(.map, string = true, array = true, null = true)
				"""#
			output: map: field1: 1
		},
	]
}
