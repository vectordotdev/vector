package metadata

remap2: literals: map: {
	title: "Map"
	description: """
		A "map" literal is a growable key/value structure.

		Maps are based on the [`BTreeMap` Rust type][b_tree_map] and syntactically equivalent to a JSON object.
		"""

	characteristics: {
		ordering: {
			title: "Ordering"
			description: """
				Maps are ordered alphabetically by the key in ascending order. Therefore, operations, such as encoding
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
