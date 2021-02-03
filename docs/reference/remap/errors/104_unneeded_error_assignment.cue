package metadata

remap: errors: "104": {
	title:       "Unneeded error assignment"
	description: """
		The left-hand side of an [assignment expression](\(urls.vrl_expressions)#\(remap.literals.regular_expression.anchor))
		needlessly handles errors when the right-hand side _cannot_ fail.
		"""
	rationale: """
		Assigning errors when one is not possible is effectively dead code that makes your program difficult to follow.
		Removing the error assignment will simplify your program.
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
			raises: compiletime: #"""
				error: \#(title)
				  ┌─ :1:1
				  │
				1 │ .message, err = downcase(.message)
				  │           ^^^
				  │           │
				  │           unneeded error assignment
				  │
				"""#
			diff: #"""
				-.message, err = downcase(.message)
				+.message = downcase(.message)
				"""#
		},
	]
}
