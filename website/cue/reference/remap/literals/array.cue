package metadata

remap: literals: array: {
	title: "Array"
	description: """
		An _array_ literal is a comma-delimited set of expressions that represents a contiguous growable array type.
		"""
	examples: [
		#"[]"#,
		#"["first", "second", "third"]"#,
		#"["mixed", 1, 1.0, true, false, {"foo": "bar"}]"#,
		#"["first-level", ["second-level", ["third-level"]]"#,
		#"[.field1, .field2, to_int!("2"), variable_1]"#,
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
