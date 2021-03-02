package metadata

remap: functions: is_nullish: {
	category: "Type"
	description: """
		Determines whether the `value` is "nullish".

		Nullish indicates the absence of a meaningful value.
		"""

	arguments: [
		{
			name:        "value"
			description: #"The value to check for "nullishness," i.e. a useless value."#
			required:    true
			type: ["any"]
		},
	]
	internal_failure_reasons: []
	return: {
		types: ["boolean"]
		rules: [
			#"If `value` is `null`, then `true` is returned."#,
			#"If `value` is `"-"`, then `true` is returned."#,
			#"If `value` is whitespace, as defined by [Unicode `White_Space` property](\#(urls.unicode_whitespace)), then `true` is returned."#,
		]
	}

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
