package metadata

remap: literals: float: {
	title: "Float"
	description: """
		A _float_ literal is a decimal representation of a 64-bit floating-point type (specifically, the "binary64" type
		defined in IEEE 754-2008).

		A decimal floating-point literal consists of an integer part (decimal digits), a decimal point, a fractional
		part (decimal digits).
		"""

	characteristics: {
		limits: {
			title: "Limits"
			description: """
				Floats in VRL can range from `-1.7976931348623157E+308f64` to `1.7976931348623157E+308f64`. Floats outside that
				range are wrapped.
				"""
		}

		underscores: {
			title: "Underscores"
			description: """
				Floats can use underscore (`_`) characters instead of `,` to make them human readable. For
				example, `1_000_000`.
				"""
		}
	}

	examples: [
		"1_000_000.01",
		"1000000.01",
		"1.001",
	]
}
