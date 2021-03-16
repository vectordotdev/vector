package metadata

remap: functions: length: {
	category: "Enumerate"
	description: """
		Returns the length of the `value`.
		"""

	arguments: [
		{
			name:        "value"
			description: "The array or object"
			required:    true
			type: ["array", "object", "string"]
		},
	]
	internal_failure_reasons: []
	return: {
		types: ["integer"]
		rules: [
			"Returns the size of the array if `value` is an array.",
			"Returns the size of the string if `value` is a string.",
			"Returns the number of map keys if `value` is a map (nested keys are ignored)",
		]
	}

	examples: [
		{
			title: "Length (object)"
			source: """
				length({
					"portland": "Trail Blazers",
					"seattle":  "Supersonics"
				})
				"""
			return: 2
		},
		{
			title: "Length (nested object)"
			source: """
				length({
					"home": {
						"city":  "Portland",
						"state": "Oregon"
					},
					"name": "Trail Blazers",
					"mascot": {
						"name": "Blaze the Trail Cat"
					}
				})
				"""
			return: 3
		},
		{
			title: "Length (array)"
			source: """
				length(["Trail Blazers", "Supersonics", "Grizzlies"])
				"""
			return: 3
		},
		{
			title: "Length (string)"
			source: """
				length("The Planet of the Apes Musical")
				"""
			return: 30
		},
	]
}
