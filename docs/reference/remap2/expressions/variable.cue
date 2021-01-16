package metadata

remap2: expressions: variable: {
	title: "Variable"
	description: """
		A "variable" expression names variables. A variable is a sequence of one or more letters and digits. The first
		character in an identifier must be a letter.
		"""
	return: """
		Returns the value of the variable.
		"""

	grammar: {
		source: """
			leading ~ (trailing)*
			"""
		definitions: {
			leading: {
				description:	"""
					The `leading` character can only be an alpha-numeric character (`a-zA-Z0-9`).
					"""
			}
			trailing: {
				description:	"""
					The `trailing` characters must only contain ASCII alpha-numeric and underscore characters
					(`a-zA-Z0-9_`).
					"""
			}
		}
	}
}
