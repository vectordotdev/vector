package metadata

remap: functions: match: {
	arguments: [
		{
			name: "value"
			description: "The value to match."
			required: true
			type: ["string"]
		},
		{
			name: "pattern"
			description: "The regular expression pattern to match against."
			required: true
			type: ["regex"]
		}
	]
	return: ["bool"]
	category: "text"
	description: """
		Determines whether a string matches the provided pattern.
		"""
	examples: [
		{
			title: "Successful match"
			input: {
				value: "I'm a little teapot"
				pattern: "/teapot/"
			}
			source: ".matches = match(.value, .pattern)"
			output: {
				matches: true
			}
		},
		{
			title: "Unsuccessful match"
			input: {
				value: "life is but a dream"
				pattern: "/teapot/"
			}
			source: ".matches = match(.value, .pattern)"
			output: {
				matches: false
			}
		}
	]
}
