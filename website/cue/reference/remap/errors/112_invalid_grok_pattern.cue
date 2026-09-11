package metadata

remap: errors: "112": {
	title:       "Invalid grok pattern"
	description: """
		A [grok](\(urls.grok)) pattern passed to `parse_grok` or `parse_groks` is invalid and cannot
		be compiled.
		"""
	resolution:  """
		Correct the grok pattern syntax. Refer to the [grok pattern documentation](\(urls.grok)) for
		valid pattern syntax and available built-in patterns.
		"""

	examples: [
		{
			"title": "\(title)"
			source: #"""
				parse_grok!(.message, "%{NOTAPATTERN:field}")
				"""#
			diff: #"""
				-parse_grok!(.message, "%{NOTAPATTERN:field}")
				+parse_grok!(.message, "%{WORD:field}")
				"""#
		},
	]
}
