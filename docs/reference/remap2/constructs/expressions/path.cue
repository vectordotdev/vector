package metadata

remap2: constructs: expressions: constructs: path: {
	title: "Path"
	description:	"""
		An path expression is a sequence of period-delimited segments that represent the location of a value
		within a map.
		"""

	examples: [
		".",
		".message",
		".parent.child",
		".grand_parent.(parent1 | parent2).child",
		".array[0]",
		".array[1]",
		".parent.child[0]",
		".\"parent.key.with.special characters\".child"
	]

	characteristics: {
		array_elements: {
			title: "Array element paths"
			description:	"""
				Array elements can be accessed by their index. Negative indice are currently _not_ supported:

				```vrl
				.array[0]
				```
				"""
		}
		coalescing: {
			title: "Path segment coalecing"
			description:	"""
				Path segments can be coalesced, allowing for the first non-null values to be used. This is particularly
				useful when working with [externally tagged](\(urls.externally_tagged_representation)) data:

				```vrl
				.grand_parent.(parent1 | parent2).child
				```
				"""
		}
		nested_maps: {
			title: "Nested map paths"
			description:	"""
				Nested map values are accessed by delimiting each ancestor path with `.`:

				```vrl
				.parent.child
				```
				"""
		}
		quoting: {
			title: "Path quoting"
			description:	"""
				Path segments can be quoted to include special characters, such as spaces, periods, and others:

				```vrl
				.\"parent.key.with.special characters\".child
				```
				"""
		}
		root: {
			title: "Path root"
			description:	"""
				The root of the event is represented by the `.` character. Therefore, _all_ paths must begin with
				the `.` character, and `.` alone is valid:

				```vrl
				. # root
				```
				"""
		}
		valid_characters: {
			title: "Valid path characters"
			description:	"""
				Path segments only allow for underscores and ASCII alpha-numeric characters (`[a-zA-Z0-9_]`). Segments
				must be delimited with periods (`.`). If a segment contains characters outside of this list it must be
				quoted.
				"""
		}
	}
}
