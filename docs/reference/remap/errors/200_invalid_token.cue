package metadata

remap: errors: "200": {
	title:       "Invalid token"
	description: """
		Your VRL program contains an invalid character.
		"""
	rationale:   """
		TODO
		"""
	resolution: """
		Use only supported characters in VRL programs.
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
