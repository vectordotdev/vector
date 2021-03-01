package metadata

remap: errors: "207": {
	title:       "Invalid string literal"
	description: "Your VRL program contains a string literal that the VRL parser doesn't recognize as valid."
	resolution: #"""
		Make sure that your string is properly enclosed by single or double quotes.
		"""#
	examples: [
		{
			"title": "\(title)"
			source: #"""
				"Houston, we have a problem'
				"""#
			diff: #"""
				- "Houston, we have a problem'
				+ "Houston, we have a problem"
				"""#
		},
	]
}
