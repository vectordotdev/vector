package metadata

remap2: constructs: expressions: constructs: assignment: {
	title: "Assignment"
	description:	"""
		An assignment expression contains left-hand and right-hand side expressions delimited by an equal operator (`=`).
		The left-hand side expression is assigned the result of the right-hand side expression.
		"""

	examples: [
		#"""
		.message = "Hello, World!"
		"""#,
		#"""
		.parent.child = "Hello, World!"
		"""#,
		#"""
		.array[1] = "Hello, World!"
		"""#,
		#"""
		my_variable = "Hello, World!"
		"""#
	]

	characteristics: {
		left_side: {
			title: "Left-hand side assignment expression"
			description:	"""
				The left-hand side of an assignment expression must be a [path expression](\(constructs.path.anchor) or
				a [variable expression](\(constructs.variable.anchor):

				```vrl
				.path = 1 + 1
				variable = 1 + 1
				```
				"""
		}
		operators: {
			title: "Assignment operators"
			description:	"""
				Assignment operators allow for condition and non-conditional assignments:

				| Operator | Description |
				|:---------|:------------|
				| `=`      | Simple assignment operator. Assigns the result from the left-hand side to the right-hand side. |
				| `=??`    | Assigns _only_ if the right hand side does not error. Useful invoking fallible functions. |
				"""

		}
		right_side: {
			title: "Right-hand side assignment expression"
			description:	"""
				The right-hand side of an assignment can be any expression.
				"""
		}
	}
}
