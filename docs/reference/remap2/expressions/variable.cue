package metadata

remap2: expressions: variable: {
	title: "Variable"
	description: """
		An "variable" expression is a sequence of period-delimited segments that represent the location of a value
		within a map.
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
