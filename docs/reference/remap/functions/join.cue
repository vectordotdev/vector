package metadata

remap: functions: join: {
	category: "String"
	description: #"""
		Joins each string in the `value` array into a single string, with items optionally separated from one another
		by a `separator`.
		"""#

	arguments: [
		{
			name:        "value"
			description: "The array of strings to join together."
			required:    true
			type: ["array"]
		},
		{
			name:        "separator"
			description: "The string separating each original element when joined."
			required:    false
			type: ["string"]
		},
	]
	internal_failure_reasons: []
	return: {
		types: ["string"]
	}

	examples: [
		{
			title: "Join array (no separator)"
			source: #"""
				join!(["bring", "us", "together"])
				"""#
			return: "bringustogether"
		},
		{
			title: "Join array (comma separator)"
			source: #"""
				join!(["sources", "transforms", "sinks"], separator: ", ")
				"""#
			return: "sources, transforms, sinks"
		},
	]
}
