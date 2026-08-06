package metadata

remap: errors: "701": {
	title: "Call to Undefined Variable"
	description: """
		The referenced variable is undefined.
		"""

	rationale: """
		Referencing a variable that is undefined results in unexpected behavior, and is likely due to a typo.
		"""

	resolution: """

		Assign the variable first, or resolve the reference typo.
		"""

	examples: [
		{
			"title": "Undefined variable"
			source: #"""
				my_variable
				"""#
			diff: #"""
				+my_variable = true
				my_variable
				"""#
		},
		{
			"title": "Wrong variable name"
			source: #"""
				my_variable = true
				my_var
				"""#
			diff: #"""
				-my_var
				+my_variable
				"""#
		},
	]
}
