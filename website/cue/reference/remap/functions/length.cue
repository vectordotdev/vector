package metadata

remap: functions: length: {
	category: "Enumerate"
	// the `return` rules below aren't rendered so we copy them here
	description: """
		Returns the length of the `value`.

		* If `value` is an array, returns the number of elements.
		* If `value` is an object, returns the number of top-level keys.
		* If `value` is a string, returns the number of bytes in the string. If
		  you want the number of characters, see `strlen`.
		"""

	arguments: [
		{
			name:        "value"
			description: "The array or object."
			required:    true
			type: ["array", "object", "string"]
		},
	]
	internal_failure_reasons: []
	return: {
		types: ["integer"]
		rules: [
			"If `value` is an array, returns the number of elements.",
			"If `value` is an object, returns the number of top-level keys.",
			"If `value` is a string, returns the number of bytes in the string.",
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
