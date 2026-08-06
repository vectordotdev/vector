package metadata

remap: errors: "105": {
	title:       "Undefined function"
	description: """
		A [function call expression](\(urls.vrl_expressions)#regular-expression) invokes an
		unknown function.
		"""
	resolution: """
		This is typically due to a typo. Correcting the function name should resolve this.
		"""

	examples: [
		{
			"title": "\(title) (typo)"
			source: #"""
				parse_keyvalue(.message)
				"""#
			diff: #"""
				-parse_keyvalue(.message)
				+parse_key_value(.message)
				"""#
		},
	]
}
