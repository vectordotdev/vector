package metadata

remap: expressions: index: {
	title: "Index"
	description: """
		An _index_ expression denotes an element of an array. Array indices in VRL start at zero.
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
					zero_based: {
						title: "Zero-based indices"
						description: """
							Indexes are zero-based where `0` represents the first array element.
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
