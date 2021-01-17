package metadata

remap: expressions: index: {
	title: "Index"
	description: """
		An _index_ expression denotes the element of an array.
		"""
	return: """
		Returns the element in the position of the supplied index.
		"""

	grammar: {
		source: """
			"[" ~ index ~ "]"
			"""
		definitions: {
			index: {
				description: """
					The `index` represents the zero-based position of the element.
					"""

				characteristics: {
					negative_indexes: {
						title: "Negative indexes"
						description: """
							Negative indexes are currently _not_ supported.
							"""
					}
					zero_based: {
						title: "Zero-based indexes"
						description: """
							Indexes are zero-based where `0` represents the first array element or string character.
							"""
					}
				}
			}
		}
	}

	examples: [
		{
			title: "Array index expression"
			input: log: array: ["first", "second"]
			source: #"""
				.array[0]
				"""#
			return: "first"
		},
	]
}
