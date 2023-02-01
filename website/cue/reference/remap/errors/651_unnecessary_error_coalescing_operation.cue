package metadata

remap: errors: "651": {
	title: "Unnecessary error coalescing operation"
	description: """
		You've used a coalescing operation (`??`) to handle an error, but in this case the left-hand
		operation is infallible, and so the right-hand value after `??` is never reached.
		"""
	rationale: """
		Error coalescing operations are useful when you want to specify what happens if an operation
		fails. Here's an example:

		```coffee
		result = op1 ?? op2
		```

		In this example, if `op1` is infallible (that is, it can't error) then the `result` variable
		if set to the value of `op1` while `op2` is never reached.
		"""
	resolution: """
		If the left-hand operation is meant to be infallible, remove the `??` operator and the
		right-hand operation. If, however, the left-hand operation is supposed to be fallible,
		remove the `!` from the function call and anything else that's making it infallible.
		"""
}
