package metadata

remap: literals: integer: {
	title: "Integer"
	description: """
		An _integer_ literal is a sequence of digits representing a 64-bit signed integer type.

		The maximum value for integers in VRL is 9223372036854775807. If an integer exceeds that,
		the value is wrapped.
		"""

	examples: [
		"1",
		"100",
	]
}
