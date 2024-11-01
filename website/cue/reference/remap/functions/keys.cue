package metadata

remap: functions: keys: {
	category: "Enumerate"
	description: #"""
		Returns the keys from the object passed into the function.
		"""#

	arguments: [
		{
			name:        "value"
			description: "The object to extract keys from."
			required:    true
			type: ["object"]
		},
	]
	internal_failure_reasons: []
	return: {
		types: ["array"]
		rules: [
			#"Returns an array of all the keys"#,
		]
	}
	examples: [
		{
			title: "Get keys from the object"
			input: log: {
				"key1": "val1"
				"key2": "val2"
			}
			source: #"""
				keys({"key1": "val1", "key2": "val2"})
				"""#
			return: ["key1", "key2"]
		},
	]
}
