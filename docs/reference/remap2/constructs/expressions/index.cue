package metadata

remap2: constructs: expressions: constructs: index: {
	title: "Index"
	description:	"""
		An index expression denotes the element of an array or a character in a string. The index mut be wrapped in
		`[` and `]` characters.
		"""

	examples: [
		".message[0]",
		".array[0]",
	]

	characteristics: {
		negative_indexes: {
			title: "Negative indexes"
			description:	"""
				Negative indexes are currently _not_ supported.
				"""
		}
		zero_based: {
			title: "Zero-based indexes"
			description:	"""
				Indexes are zero-based where `0` represents the first array element or string character.
				"""
		}
	}
}
