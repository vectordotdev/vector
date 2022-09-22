package metadata

remap: functions: values: {
	category: "Enumerate"
	description: #"""
		Returns the values from the object passed into the function.
		"""#

	arguments: [
		{
			name:        "value"
			description: "The key, value object to extract values from."
			required:    true
			type: ["object"]
		},
	]
	internal_failure_reasons: []
	return: {
		types: ["array"]
		rules: [
			#"Returns an array for all the values."#,
		]
	}
	examples: [
		{
			title: "Get values from key value object"
			input: log: {
				"key3": "val3"
				"key4": "val4"
			}
			source: #"""
				values({"key1": "val1", "key2": "val2"})
				"""#
			return: ["val1", "val2"]
		},
	]
}
