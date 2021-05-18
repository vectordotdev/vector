package metadata

remap: literals: object: {
	title: "Object"
	description: """
		An _object_ literal is a growable key/value structure that is syntactically equivalent to a JSON object.

		A well-formed JSON document is a valid VRL object.
		"""

	characteristics: {
		ordering: {
			title: "Ordering"
			description: """
				Object fields are ordered alphabetically by the key in ascending order. Therefore, operations like
				encoding into JSON produce a string with keys that are in ascending alphabetical order.
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
		"""
			{
				"field1": .some_path,
				"field2": some_variable,
				"field3": { "subfield": "some value" }
			}
			""",

	]
}
