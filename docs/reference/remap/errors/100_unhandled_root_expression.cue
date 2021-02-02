package metadata

remap: errors: "100": {
	title: "Unhandled root expression"
	description: """
		A root expression is effectively dead code; it does not change the result of your program.
		"""
	rationale: """
		Dead code is unecessary and needlessly contributes to the execution time of your program. Removing it will
		make your program simpler and faster.
		"""
	resolution: """
		This error is usually accidental and caused by improper white-space usage. To resolve this error, use the
		expression in a way that contributes to your program's result, or remove the expression.
		"""

	examples: [
		{
			"title": "\(title) (bad comment)"
			source: #"""
				# .new_key = 100 + \
					(5 / 2)
				"""#
			raises: compiletime: #"""
				error: \#(title)
				  ┌─ :1:1
				  │
				1 │ 	(5 / 2)
				  │     ^^^^^
				  │     │
				  │     this expression is unhandled
				  │
				"""#
			diff: #"""
				 # .new_key = 100 + \
				- 	(5 / 2)
				+# 	(5 / 2)
				"""#
		},
	]
}
