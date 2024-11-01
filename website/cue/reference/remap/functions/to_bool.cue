package metadata

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
		"`value` is not a supported boolean representation.",
	]
	return: {
		types: ["boolean"]
		rules: [
			#"If `value` is `"true"`, `"t"`, `"yes"`, or `"y"`, `true` is returned."#,
			#"If `value` is `"false"`, `"f"`, `"no"`, `"n"`, or `"0"`, `false` is returned."#,
			#"If `value` is `0.0`, `false` is returned, otherwise `true` is returned."#,
			#"If `value` is `0`, `false` is returned, otherwise `true` is returned."#,
			#"If `value` is `null`, `false` is returned."#,
			#"If `value` is a Boolean, it's returned unchanged."#,
		]
	}

	examples: [
		{
			title: "Coerce to a Boolean (string)"
			source: """
				to_bool!("yes")
				"""
			return: true
		},
		{
			title: "Coerce to a Boolean (float)"
			source: """
				to_bool(0.0)
				"""
			return: false
		},
		{
			title: "Coerce to a Boolean (int)"
			source: """
				to_bool(0)
				"""
			return: false
		},
		{
			title: "Coerce to a Boolean (null)"
			source: """
				to_bool(null)
				"""
			return: false
		},
		{
			title: "Coerce to a Boolean (Boolean)"
			source: """
				to_bool(true)
				"""
			return: true
		},
	]
}
