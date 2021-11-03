package metadata

remap: errors: "104": {
	title:       "Unnecessary error assignment"
	description: """
		The left-hand side of an [assignment expression](\(urls.vrl_expressions)#regular-expression)
		needlessly handles errors even though the right-hand side _can't_ fail.
		"""
	rationale: """
		Assigning errors when one is not possible is effectively dead code that makes your program difficult to follow.
		Removing the error assignment simplifies your program.
		"""
	resolution: """
		Remove the error assignment.
		"""

	examples: [
		{
			"title": "\(title) (strings)"
			source: #"""
				.message, err = downcase(.message)
				"""#
			diff: #"""
				-.message, err = downcase(.message)
				+.message = downcase(.message)
				"""#
		},
	]
}
