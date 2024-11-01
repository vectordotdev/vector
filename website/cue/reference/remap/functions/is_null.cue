package metadata

remap: functions: is_null: {
	category:    "Type"
	description: """
		Check if `value`'s type is `null`. For a more relaxed function,
		see [`is_nullish`](\(urls.vrl_functions)#\(remap.functions.is_nullish.anchor)).
		"""

	arguments: [
		{
			name:        "value"
			description: #"The value to check if it is `null`."#
			required:    true
			type: ["any"]
		},
	]
	internal_failure_reasons: []
	return: {
		types: ["boolean"]
		rules: [
			#"Returns `true` if `value` is null."#,
			#"Returns `false` if `value` is anything else."#,
		]
	}

	examples: [
		{
			title: "Null value"
			source: """
				is_null(null)
				"""
			return: true
		},
		{
			title: "Non-matching type"
			source: """
				is_null("a string")
				"""
			return: false
		},
	]
}
