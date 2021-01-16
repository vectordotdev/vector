package metadata

remap2: constructs: expressions: constructs: if: {
	title: "If"
	description:	"""
		An if expression allows for conditional control-flow where the return of the expressions is the result of
		the last expression evaluated.
		"""

	examples: [
		"""
		if (true) {
			"this is returned"
		} else if (true) {
			"this is not returned"
		} else {
			"this is also not returned"
		}
		"""
	]
}
