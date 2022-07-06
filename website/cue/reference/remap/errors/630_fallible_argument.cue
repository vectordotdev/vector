package metadata

remap: errors: "630": {
	title: "Fallible argument"
	description: """
		You've passed a fallible expression as an argument to a function.
		"""

	rationale: """
		In VRL, expressions that you pass to functions as arguments need to be infallible themselves. Otherwise, the
		outcome of the function would be indeterminate.
		"""

	resolution: """
		Make the expression passed to the function infallible, potentially by aborting on error using `!`, coalescing
		the error using `??`, or via some other method.
		"""

	examples: [
		{
			"title": "\(title)"
			source: #"""
				format_timestamp!(to_timestamp("2021-01-17T23:27:31.891948Z"), format: "%v %R")
				"""#
			diff: #"""
				- 	format_timestamp!(to_timestamp("2021-01-17T23:27:31.891948Z"), format: "%v %R")
				+ 	format_timestamp!(to_timestamp!("2021-01-17T23:27:31.891948Z"), format: "%v %R")
				"""#
		},
	]
}
