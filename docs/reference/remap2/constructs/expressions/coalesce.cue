package metadata

remap2: constructs: expressions: constructs: coalesce: {
	title: "Coalesce"
	description:	"""
		A coalesce expression is composed of multiple expressions delimited by a coalesce operator, returning the
		result of the first expression that does not violate the operator condition.
		"""

	examples: [
		#"""
		parse_json(.message) ?? parse_apache_log(.message) ?? "failed"
		"""#,
	]

	characteristics: {
		arguments: {
			title: "Coalesce operators"
			description:	"""
				Coalesce operators allow coalecing on specified conditions:

				| Operator | Description |
				|:---------|:------------|
				| `??`     | Error coalescing. Returns the first expression that does not error. |
				"""
		}
	}
}
