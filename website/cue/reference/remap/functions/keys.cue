package metadata

remap: functions: values: {
	category: "Type"
	description: """
		Returns the `values` from the object passed into the function
		"""

	arguments: [
		{
			name:        "value"
			description: "The value that you need to ensure is an object containing keys and values."
			required:    true
			type: ["any"]
		},
	]
	return: {
		types: ["array"]
		rules: [
			#"Returns an array for all the values"#,
		]
	}
	examples: [
		{
			title: "Get keys from key value object"
			input: log: value:
			source: #"""
				values({"key1": "val1", "key2": "val2"})
				"""#
			return: ["val1", "val2"]
		},
	]
}
