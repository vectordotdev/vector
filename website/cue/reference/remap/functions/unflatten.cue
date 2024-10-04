package metadata

remap: functions: unflatten: {
	category: "Enumerate"
	description: #"""
		Unflattens the `value` into a nested representation.
		"""#

	arguments: [
		{
			name:        "value"
			description: "The array or object to unflatten."
			required:    true
			type: ["object"]
		},
		{
			name:        "separator"
			description: "The separator to split flattened keys."
			required:    false
			default:     "."
			type: ["string"]
		},
		{
			name:        "recursive"
			description: "Whether to recursively unflatten the object values."
			required:    false
			default:     "true"
			type: ["boolean"]
		},
	]
	internal_failure_reasons: []
	return: types: ["object"]

	examples: [
		{
			title: "Unflatten"
			source: #"""
				unflatten({
				    "foo.bar.baz": true,
				    "foo.bar.qux": false,
					"foo.quux": 42
				})
				"""#
			return: {
				"foo": {
					"bar": {
						"baz": true
						"qux": false
					}
					"quux": 42
				}
			}
		},
		{
			title: "Unflatten recursively"
			source: #"""
				unflatten({
				    "flattened.parent": {
						"foo.bar": true,
						"foo.baz": false
					}
				})
				"""#
			return: {
				"flattened": {
					"parent": {
						"foo": {
							"bar": true
							"baz": false
						}
					}
				}
			}
		},
		{
			title: "Unflatten non-recursively"
			source: #"""
				unflatten({
				    "flattened.parent": {
						"foo.bar": true,
						"foo.baz": false
					}
				}, recursive: false)
				"""#
			return: {
				"flattened": {
					"parent": {
						"foo.bar": true
						"foo.baz": false
					}
				}
			}
		},
		{
			title: "Ignore inconsistent keys values"
			source: #"""
				unflatten({
					"a": 3,
					"a.b": 2,
					"a.c": 4
				})
				"""#
			return: {
				"a": {
					"b": 2
					"c": 4
				}
			}
		},
	]
}
