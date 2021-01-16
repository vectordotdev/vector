package metadata

remap2: expressions: variable: {
	title: "Variable"
	description: """
		An path expression is a sequence of period-delimited segments that represent the location of a value
		within a map.
		"""
	return: """
		Returns the value of the variable.
		"""

	grammar: {
		source: """
			!(reserved_keyword ~ !(ASCII_ALPHANUMERIC | "_")) ~ ASCII_ALPHANUMERIC ~ (ASCII_ALPHANUMERIC | "_")*
			"""
		definitions: {}
	}
}
