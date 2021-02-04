package metadata

remap: literals: integer: {
	title: "Integer"
	description: """
		An _integer_ literal is a sequence of digits representing a 64-bit signed integer type.
		"""

	characteristics: {
		human_readable: {
			title: "Human readable"
			description: """
				Integers can leverage `_` characters, instead of `,`, to make them human readable. For example,
				`1_000_000`.
				"""
		}

		ordering: {
			title: "Limits"
			description: """
				Integers in VRL can range from `-9223372036854775807` to `9223372036854775807`. Integers outside that range are
				wrapped.
				"""
		}
	}

	examples: [
		"1_000_000",
		"1000000",
	]
}
