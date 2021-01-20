package metadata

remap: functions: is_nullish: {
	arguments: [
		{
			name:        "value"
			description: #"The value to check for "nullishness," i.e. a useless value."#
			required:    true
			type: ["any"]
		},
	]
	internal_failure_reasons: []
	return: ["boolean"]
	category: "Type"
	description: #"""
		Determines whether the provided `value` is "nullish,"

		Nullish indicates the absence of a meaningful value. The following are considered nullish in VRL:

		* `null`
		* `"-"` (A single dash)
		* Whitespace as defined by [Unicode `White_Space` property](\(urls.unicode_whitespace))
		"""#
	examples: [
		{
			title: "Null detection (blank string)"
			source: """
				is_nullish("")
				"""
			return: true
		},
		{
			title: "Null detection (dash string)"
			source: """
				is_nullish("-")
				"""
			return: true
		},
		{
			title: "Null detection (whitespace)"
			source: """
				is_nullish("\n  \n")
				"""
			return: true
		},
	]
}
