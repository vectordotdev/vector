package metadata

remap: expressions: coalesce: {
	title: "Coalesce"
	description: """
		A _coalesce_ expression is composed of multiple expressions (operands) delimited by a coalesce operator,
		short-circuiting on the first expression that does not violate the operator condition.
		"""
	return: """
		Returns the value of the first expression (operand) that does not violate the operator condition.
		"""

	grammar: {
		source: """
			expression ~ (operator ~ expression)+
			"""
		definitions: {
			expression: {
				description: """
					The `expression` (operand) can be any expression.
					"""
			}
			operator: {
				description: """
					The `operator` delimits two or more `expression`s.
					"""
				enum: {
					"??": """
						The `??` operator performs error coalescing, short-circutiing on the first expression that does not
						error and returning its result.
						"""
				}
			}
		}
	}

	examples: [
		{
			title: "Error coalescing"
			source: #"""
				parse_syslog("not syslog") ?? parse_apache_log("not apache") ?? "malformed"
				"""#
			return: "malformed"
		},
	]
}
