remap: functions: to_bool: {
	category: "Coerce"
	description: """
		Coerces the `value` into a boolean.
		"""

	arguments: [
		{
			name:        "value"
			description: "The value to convert to a Boolean."
			required:    true
			type: ["boolean", "integer", "float", "null", "string"]
		},
	]
	internal_failure_reasons: [
		"`value` is not a supported boolean representation",
	]
	return: {
		types: ["boolean"]
		rules: [
			#"If `value` is `"true"`, `"t"`, `"yes"`, `"y"` then `true` is returned."#,
			#"If `value` is `"false"`, `"f"`, `"no"`, `"n"`, `"0"` then `false` is returned."#,
			#"If `value` is `0.0` then `false` is returned, otherwise `true` is returned."#,
			#"If `value` is `0` then `false` is returned, otherwise `true` is returned."#,
			#"If `value` is `null` then `false` is returned."#,
			#"If `value` is a boolean then it is passed through."#,
		]
	}

	examples: [
		{
			title: "Coerce to a boolean (string)"
			source: """
				to_bool("yes")
				"""
			return: true
		},
		{
			title: "Coerce to a boolean (float)"
			source: """
				to_bool(0.0)
				"""
			return: false
		},
		{
			title: "Coerce to a boolean (int)"
			source: """
				to_bool(0)
				"""
			return: false
		},
		{
			title: "Coerce to a boolean (null)"
			source: """
				to_bool(null)
				"""
			return: false
		},
		{
			title: "Coerce to a boolean (boolean)"
			source: """
				to_bool(true)
				"""
			return: true
		},
	]
}
