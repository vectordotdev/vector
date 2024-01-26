package metadata

remap: expressions: block: {
	title: "Block"
	description: """
		A _block_ expression is a sequence of one or more expressions within matching brace brackets.

		Blocks can't be empty. Instead, empty blocks (`{}`) are treated as blank objects.
		"""
	return: """
		Returns the result of the last evaluated expression within the block.
		"""

	grammar: {
		source: """
			"{" ~ NEWLINE* ~ expressions ~ NEWLINE* ~ "}"
			"""
		definitions: {
			expressions: {
				description: """
					One or more expressions.
					"""
			}
		}
	}

	examples: [
		{
			title: "Simple block"
			source: #"""
				{
					message = "{\"Hello\": \"World!\"}"
					parse_json!(message)
				}
				"""#
			return: Hello: "World!"
		},
		{
			title: "Assignment block"
			source: #"""
				.structured = {
					message = "{\"Hello\": \"World!\"}"
					parse_json!(message)
				}
				"""#
			output: log: structured: Hello: "World!"
		},
	]
}
