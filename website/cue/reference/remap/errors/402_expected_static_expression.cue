package metadata

remap: errors: "402": {
	title: "Expected static expression for function argument"
	description: """
		VRL expected a static expression for a function argument, but a dynamic one was provided (such as a variable).

		VRL requires static expressions for some function arguments to validate argument types at
		compile time to avoid runtime errors.
		"""
	resolution: """
		Replace the dynamic argument with a static expression.
		"""
}
