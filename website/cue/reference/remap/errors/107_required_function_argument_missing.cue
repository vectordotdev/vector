package metadata

remap: errors: "107": {
	title:       "Required function argument missing"
	description: """
		A [function call expression](\(urls.vrl_expressions)#regular-expression) fails to pass
		a required argument.
		"""
	resolution: """
		Supply all of the required function arguments to adhere to the function's documented signature.
		"""

	examples: [
		{
			"title": title
			source: #"""
				parse_timestamp(.timestamp)
				"""#
			diff: #"""
				-parse_timestamp(.timestamp)
				+parse_timestamp(.timestamp, format: "%D")
				"""#
		},
	]
}
