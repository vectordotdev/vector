package metadata

remap: functions: length: {
	arguments: [
		{
			name:        "value"
			description: "The array or map"
			required:    true
			type: ["array", "map", "string"]
		},
	]
	internal_failure_reasons: []
	return: ["integer"]
	category: "Enumerate"
	description: """
		Returns the length of the input:

		* If an array, the size of the array
		* If a string, the number of bytes in the string
		* If a map, the number of keys in the map (nested keys are ignored)
		"""
	examples: [
		{
			title: "Length (map)"
			source: """
				length({
					"portland": "Trail Blazers"
					"seattle":  "Supersonics"
				})
				"""
			return: 2
		},
		{
			title: "Length (nested map)"
			source: """
				length({
					"home": {
						"city":  "Portland"
						"state": "Oregon"
					}
					"name": "Trail Blazers"
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
