package metadata

remap: functions: to_string: {
	arguments: [
		{
			name:        "value"
			description: "The value to return a string representation of."
			required:    true
			type: ["any"]
		},
	]
	internal_failure_reasons: []
	return: ["string"]
	category: "Coerce"
	description: #"""
		Coerces the provided `value` into a `string`.
		"""#
	examples: [
		{
			title: "Coerce to a string (boolean)"
			source: #"""
				to_string(true)
				"""#
			return: true
		},
		{
			title: "Coerce to a string (int)"
			source: #"""
				to_string(52)
				"""#
			return: "52"
		},
		{
			title: "Coerce to a string (float)"
			source: #"""
				to_string(52.2)
				"""#
			return: "52.2"
		},
	]
}
