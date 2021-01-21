package metadata

remap: literals: string: {
	title:       "String"
	description: """
		A "string" literal is a [UTF-8–encoded](\(urls.utf8)), growable string.
		"""

	examples: [
		#"""
			"Hello, world!"
			"""#,
		#"""
			"Hello �world!"
			"""#,
		#"""
			"💖"
			"""#,
	]

	characteristics: {
		concatenation: {
			title: "Concatenation"
			description: """
				Strings can be concatenated with the `+` operator.
				"""
		}
		invalid_characters: {
			title: "Invalid Characters"
			description: """
				Invalid UTF-8 sequences are replaced with the `�` character.
				"""
		}
	}
}
