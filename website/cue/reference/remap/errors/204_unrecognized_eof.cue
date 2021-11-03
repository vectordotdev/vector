package metadata

remap: errors: "204": {
	title: "Unrecognized end-of-file (EOF)"
	description: """
		The VRL parser has reached the end of the program in an invalid state, potentially due to a
		typo or a dangling expression.
		"""
	resolution: """
		Make sure that the last expression in the program is valid.
		"""
	examples: [
		{
			"title": "\(title)"
			source: #"""
				.field1 = "value1"
				.field2 =
				"""#
			diff: #"""
				-.bar =
				+.field2 = "value2"
				"""#
		},
	]
}
