package metadata

remap: literals: integer: {
	title: "Integer"
	description: """
		An _integer_ literal is a sequence of digits representing a 64-bit signed integer type.

		Integers in VRL can range from -9223372036854775807 to 9223372036854775807. Integers outside that range are
		wrapped.
		"""

	examples: [
		"1",
		"100",
	]
}
