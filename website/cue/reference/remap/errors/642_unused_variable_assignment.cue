package metadata

remap: errors: "642": {
	title: "Unused Variable Assignment"
	description: """
		You've assigned a value to a variable that is never used in the
		program.
		"""

	rationale: """
		All variable assignments in VRL need to have at least one reference. If you fail to reference a variable, it
		likely means you forgot to use the variable, or referenced the wrong variable. Preventing this from passing
		compilation makes sure you don't accidentally end up in an unexpected situation at runtime.
		"""

	resolution: """
		Reference the variable to resolve this error.
		
		You can optionally use a no-op assignment (`_`) or prepend your variable name with a `_` (e.g. `_foo`), if you
		want to force the compiler to accept your unused variable (f.e. during debugging/testing purposes).
		"""

	examples: [
		{
			title: "Unused variable assignment"
			source: #"""
				my_variable = true
				"""#
			diff: #"""
				+my_variable
				"""#
		},
		{
			title: "Unused error coalescing variable"
			source: #"""
				ok, err = 1 / 0
				err
				"""#
			diff: #"""
				-ok, err = 1 / 0
				+_, err = 1 / 0
				"""#
		},
		{
			title: "Silence variable assignment error"
			source: #"""
				my_variable = true
				"""#
			diff: #"""
				-my_variable = true
				+_my_variable = true
				"""#
		},
	]
}
