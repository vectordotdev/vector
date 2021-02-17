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
				Objects are ordered alphabetically by the key in ascending order. Therefore, operations, such as
				encoding into JSON, will produce a string with keys that are alphabetically ordered in ascending
				fashion.
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
