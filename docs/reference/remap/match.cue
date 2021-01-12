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
	internal_failure_reasons: []
	return: ["boolean"]
	category: "String"
	description: """
		Returns `true` if the provided `value` matches the provided `pattern`.
		"""
	examples: [
		{
			title: "Successful Regex match on string"
			input: log: phrase: "I'm a little teapot"
			source: ".has_teapot = match(.phrase, /teapot/)"
			output: input & {log: has_teapot: true}
		},
		{
			title: "Unsuccessful Regex match on string"
			input: log: phrase: "Life is but a dream"
			source: ".has_teapot = match(.phrase, /teapot/)"
			output: input & {log: has_teapot: false}
		},
	]
}
