package metadata

remap: errors: "201": {
	title:       "Extra token"
	description: """
		Your VRL program contains
		"""
	rationale:   """
		TODO
		"""
	resolution: """
		TODO
		"""

	examples: [
		{
			"title": title
			source: #"""
				😀
				"""#
			raises: compiletime: #"""
				error: \#(title)
				┌─ :1:1
				│
				1 │ 😀
				│ ^^
				│ │
				│ unexpected syntax token: "InvalidToken"
				│ expected one of: "\n", "!", "(", "[", "_", "false", "float literal", "function call", "identifier", "if", "integer literal", "null", "regex literal", "string literal", "timestamp literal", "true", "{", "path literal"
				"""#
			diff: #"""
				-😀
				"""#
		},
	]
}
