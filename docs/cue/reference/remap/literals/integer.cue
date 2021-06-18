package metadata

remap: literals: integer: {
	title: "Integer"
	description: """
		An _integer_ literal is a sequence of digits representing a 64-bit signed integer type.
		"""

	characteristics: {
		ordering: {
			title: "Limits"
			description: """
				Integers in VRL can range from `-9223372036854775807` to `9223372036854775807`. Integers outside that range are
				wrapped.
				"""
		}

		underscore: {
			title: "Underscore"
			description: """
				Integers can use underscore (`_`) characters instead of `,` to make them human readable. For
				example, `1_000_000`.
				"""
		}
	}

	examples: [
		"1_000_000",
		"1000000",
	]
}
