package metadata

remap: errors: "630": {
	title:       "Fallible argument"
	description: """
		In VRL, expressions that you pass to functions as arguments need to be infallible.
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
			raises: compiletime: #"""
				error: \#(title)
				┌─ :1:19
				│
				1 │ format_timestamp!(to_timestamp("2021-01-17T23:27:31.891948Z"), format: "%v %R")
				│                   ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
				│                   │
				│                   this expression can fail
				│                   handle the error before passing it in as an argument
				|
				"""#
			diff: #"""
				- 	format_timestamp!(to_timestamp("2021-01-17T23:27:31.891948Z"), format: "%v %R")
				+# 	format_timestamp!(to_timestamp!("2021-01-17T23:27:31.891948Z"), format: "%v %R")
				"""#
		},
	]
}
