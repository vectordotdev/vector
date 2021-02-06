package metadata

remap: functions: tag_types_externally: {
	category: "Type"
	description: """
		Adds type information to all (nested) scalar values in the provided `value`.

		The type information is added externally, meaning that `value` will have the shape of `"type": value` after this
		transformation.
		"""
	arguments: [
		{
			name:        "value"
			description: "The value that should be tagged with types externally."
			required:    true
			type: ["any"]
		},
	]
	internal_failure_reasons: []
	return: types: ["map"]
	examples: [
		{
			title: "Tag types externally"
			source: #"""
				tag_types_externally({
					"message": "Hello world",
					"request": {
						"duration_ms": 67.9
					}
				})
				"""#
			return: {
				message: {
					bytes: "Hello world"
				}
				request: {
					duration_ms: {
						float: 67.9
					}
				}
			}
		},
	]
}
