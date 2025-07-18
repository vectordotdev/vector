package metadata

remap: expressions: variable: {
	title: "Variable"
	description: """
		A _variable_ expression names variables. A variable is a sequence of one or more letters and digits. The first
		character in a variable must be a letter.
		"""
	return: """
		Returns the value of the variable.
		"""

	grammar: {
		source: """
			first ~ (trailing)*
			"""
		definitions: {
			first: {
				description: """
					The `first` character can only be an alpha-numeric character (`a-zA-Z0-9`).
					"""
			}
			trailing: {
				description: """
					The `trailing` characters must only contain ASCII alpha-numeric and underscore characters
					(`a-zA-Z0-9_`).
					"""
			}
		}
	}

	examples: [
		{
			title: "Simple variable"
			source: #"""
				my_variable = 1
				my_variable == 1
				"""#
			return: true
		},
		{
			title: "Variable with path"
			source: #"""
				my_object = { "one": 1 }
				my_object.one
				"""#
			return: 1
		},
	]
}
