package metadata

remap: functions: to_float: {
	category: "Coerce"
	description: """
		Coerces the `value` into a float.
		"""

	arguments: [
		{
			name:        "value"
			description: "The string that is to be converted to a float. Must be the string representation of a `float`, otherwise an `ArgumentError` will be raised."
			required:    true
			type: ["float", "integer", "boolean", "string"]
		},
	]
	internal_failure_reasons: [
		"`value` is not a supported float representation",
	]
	return: {
		types: ["float"]
		rules: [
			"If `value` is a string, it must be the string representation of an float or else an error is raised.",
			"If `value` is a boolean, `0.0` will be returned for `false` and `1.0` will be returned for `true`.",
		]
	}

	examples: [
		{
			title: "Coerce to a float"
			source: """
				to_float("3.145")
				"""
			return: 3.145
		},
	]
}
