package metadata

remap: functions: to_float: {
	category: "Coerce"
	description: """
		Coerces the `value` into a float.
		"""

	arguments: [
		{
			name: "value"
			description: """
				The value to convert to a float. Must be convertible to a float, otherwise an error is raised.
				"""
			required: true
			type: ["float", "integer", "boolean", "string"]
		},
	]
	internal_failure_reasons: [
		"`value` isn't a supported float representation",
	]
	return: {
		types: ["float"]
		rules: [
			"If `value` is a string, it must be the string representation of an float or else an error is raised.",
			"If `value` is a boolean, `0.0` is returned for `false` and `1.0` is returned for `true`.",
		]
	}

	examples: [
		{
			title: "Coerce to a float"
			source: """
				to_float!("3.145")
				"""
			return: 3.145
		},
	]
}
