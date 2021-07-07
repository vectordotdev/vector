package metadata

remap: errors: "203": {
	title: "Unrecognized token"
	description: """
		Your VRL program contains a token (character) that the VRL parser doesn't recognize as valid.
		"""
	rationale: null
	resolution: """
		Use a valid token.
		"""

	examples: [
		{
			"title": "\(title)"
			source: #"""
				😂
				"""#
			diff: #"""
				-😂
				+"some valid value"
				"""#
		},
	]
}
