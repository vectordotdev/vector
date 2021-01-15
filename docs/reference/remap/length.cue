package metadata

remap: functions: length: {
	arguments: [
		{
			name:        "value"
			description: "The array or map"
			required:    true
			type: ["array", "map"]
		},
	]
	internal_failure_reason: null
	return: ["integer"]
	category: "Enumerate"
	description: """
		Return the length of the array or the number of keys in the map (nested keys are ignored).
		"""
	examples: [
		{
			title: "Standard map"
			input: log: teams: {
				portland: "Trail Blazers"
				seattle:  "Supersonics"
			}
			source: ".num_teams = length(del(.teams))"
			output: log: num_teams: 2
		},
		{
			title: "Array"
			input: log: teams: ["Trail Blazers", "Supersonics", "Grizzlies"]
			source: ".num_teams = length(del(.teams))"
			output: log: num_teams: 3
		},
		{
			title: "Nested map"
			input: log: team: {
				city: "Portland"
				name: "Trail Blazers"
				mascot: {
					name: "Blaze the Trail Cat"
				}
			}
			source: ".num_attrs = length(del(.team))"
			output: log: num_attrs: 3
		},
	]
}
