package metadata

remap: syntax: comment: {
	title: "Comment"
	description: """
		A _comment_ serves as program documentation and is identified with `#`. Each line must be preceeded with a
		`#` character. VRL currently does not allow for block comments.
		"""

	examples: [
		"# comment",
		"""
			# multi-line
			# comment
			""",
	]
}
