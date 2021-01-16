package metadata

remap2: literals: map: {
	title: "Map"
	description: """
		A map literal is a growable key/value structure whole values are a set of expressions.

		Maps are based on the [`BTreeMap` Rust type][b_tree_map] and syntactically equivalet to a JSON object.
		"""

	characteristics: {
		ordering: {
			title: "Ordering"
			description: """
				Maps are ordered alphabetically by the key in asending order. Therefore, operations, such as encoding
				into JSON, will produce a string with keys that are alphabetically ordered in ascending fashion.
				"""
		}
	}

	examples: [
		"""
			{
				"field1": "value1",
				"field2": [ "value2", "value3", "value4" ],
				"field3": { "field4": "value5" }
			}
			""",

	]
}
