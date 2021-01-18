package metadata

remap: literals: float: {
	title: "Float"
	description: """
		A _float_ literal is a decimal representation of a 64-bit floating-point type (specifically, the "binary64" type
		defined in IEEE 754-2008).

		A decimal floating-point literal consists of an integer part (decimal digits), a decimal point, a fractional
		part (decimal digits).

		The maximum value for floags in VRL is 1.7976931348623157E+308f64. If a float exceeds that, the value is wrapped.
		"""

	examples: [
		"1.0",
		"1.01",
		"1.001",
	]
}
