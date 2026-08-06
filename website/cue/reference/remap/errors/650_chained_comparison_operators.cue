package metadata

remap: errors: "650": {
	title: "Chained comparison operators"

	description: """
		You've chained multiple comparison operators together in a way that can't result in a valid expression.
		"""

	rationale: """
		Comparison operators can only operate on two operands, e.g. `1 != 2`. Chaining them together, as in
		`1 < 2 < 3`, produces a meaningless non-expression.
		"""

	resolution: """
		Use comparison operators only on a left-hand- and a right-hand-side value. You *can* chain comparisons together
		provided that the expressions are properly grouped. While `a == b == c`, for example, isn't valid,
		`a == b && b == c` *is* valid because it involves distinct Boolean expressions.
		"""

	examples: [
		{
			"title": "\(title)"
			source: #"""
				1 == 1 == 2
				"""#
			diff: #"""
				- 	1 == 1 == 2
				+# 	(1 == 1) && (1 == 2)
				"""#
		},
	]
}
