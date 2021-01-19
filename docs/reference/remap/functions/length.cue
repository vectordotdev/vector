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
			title: "Standard map"
			input: log: teams: {
				portland: "Trail Blazers"
				seattle:  "Supersonics"
			}
			source: ".num_teams = length(.teams)"
			output: input & {log: num_teams: 2}
		},
		{
			title: "Array"
			input: log: teams: ["Trail Blazers", "Supersonics", "Grizzlies"]
			source: ".num_teams = length(.teams)"
			output: input & {log: num_teams: 3}
		},
		{
			title: "Nested map"
			input: log: team: {
				home: {
					city:  "Portland"
					state: "Oregon"
				}
				name: "Trail Blazers"
				mascot: {
					name: "Blaze the Trail Cat"
				}
			}
			source: ".num_attrs = length(.team)"
			output: input & {log: num_attrs: 3}
		},
		{
			title: "String"
			input: log: message: "The Planet of the Apes Musical"
			source: ".str_len = length(.message)"
			output: input & {log: str_len: 30}
		},
	]
}
