package metadata

remap2: expressions: block: {
	title: "Assignment"
	description: """
		An "block" expression is a possibly empty sequence of expressions within matching brace brackets.
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
				description:	"""
					One or more expresions.
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
			return: Hello: "World!"
			output: log: structured: return
		},
	]
}
