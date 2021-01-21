package metadata

remap: literals: array: {
	title: "Array"
	description: """
		An _array_ literal is a comma-delimited set of expressions that represent a contiguous growable array type.
		"""
	examples: [
		#"["first", "second", "third"]"#,
		#"["mixed", 1, 1.0, true, false, {"foo": "bar"}]"#,
		#"["first-level", ["second-level", ["third-level"]]"#,
		#"""
			[
				"expressions",
				1 + 2,
				2 == 5,
				true || false
			]
			"""#,
	]
}
