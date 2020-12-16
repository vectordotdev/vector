package metadata

remap: functions: match: {
	arguments: [
		{
			name:        "value"
			description: "The value to match."
			required:    true
			type: ["string"]
		},
		{
			name:        "pattern"
			description: "The regular expression pattern to match against."
			required:    true
			type: ["regex"]
		},
	]
	return: ["boolean"]
	category: "text"
	description: """
		Determines whether a string matches the provided pattern.
		"""
	examples: [
		{
			title: "Successful match"
			input: {
				phrase: "I'm a little teapot"
			}
			source: ".has_teapot = match(.phrase, /teapot/)"
			output: {
				has_teapot: true
			}
		},
		{
			title: "Unsuccessful match"
			input: {
				phrase: "life is but a dream"
			}
			source: ".has_teapot = match(.phrase, /teapot/)"
			output: {
				has_teapot: false
			}
		},
	]
}
