package metadata

remap: expressions: coalesce: {
	title: "Coalesce"
	description: """
		A _coalesce_ expression is composed of multiple expressions (operands) delimited by a coalesce operator,
		short-circuiting on the first expression that doesn't violate the operator condition.
		"""
	return: """
		Returns the value of the first expression (operand) that doesn't violate the operator condition.
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
						The `??` operator performs error coalescing, short-circuiting on the first expression that
						doesn't error and returning its result.
						"""
				}
			}
		}
	}

	examples: [
		{
			title: "Error coalescing"
			source: #"""
				parse_syslog("not syslog") ?? parse_common_log("not common") ?? "malformed"
				"""#
			return: "malformed"
		},
	]
}
