package metadata

remap: errors: "203": {
	title:       "Unrecognized token"
	description: """
		Your VRL program contains a token (character) that the VRL parses doesn't recognize as valid.
		"""
	rationale: null
	resolution:  """
		Use a valid token.
		"""

	examples: [
		{
			"title": "\(title)"
			source: #"""
				😂
				"""#
			raises: compiletime: #"""
				error: \#(title)
				  ┌─ :1:1
				  │
				1 │ 😂
				  │ ^^
				  │ │
				  │ unexpected syntax token: "InvalidToken"
				  │ expected one of: "\n", "!", "(", "[", "_", "false", "float literal", "function call", "identifier", "if", "integer literal", "null", "regex literal", "string literal", "timestamp literal", "true", "{", "path literal"
				  │
				"""#
			diff: #"""
				-😂
				"""#
		},
	]
}
