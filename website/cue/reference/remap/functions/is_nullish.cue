package metadata

remap: functions: is_nullish: {
	category: "Type"
	description: """
		Determines whether `value` is nullish. Returns `true` if the specified `value` is `null`,
		an empty string, a string containing only whitespace, or the string `"-"`. Returns `false` otherwise.
		"""

	notices: [
		"""
			This function behaves inconsistently: it returns `false` for empty arrays (`[]`) and objects (`{}`),
			but `true` for empty strings (`""`) and `null`.
			""",
	]

	arguments: [
		{
			name:        "value"
			description: #"The value to check for nullishness, for example, a useless value."#
			required:    true
			type: ["any"]
		},
	]
	internal_failure_reasons: []
	return: {
		types: ["boolean"]
		rules: [
			#"Returns `true` if `value` is `null`."#,
			#"Returns `true` if `value` is `"-"`."#,
			#"Returns `true` if `value` is whitespace as defined by [Unicode `White_Space` property](\#(urls.unicode_whitespace))."#,
			#"Returns `false` if `value` is anything else."#,
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
