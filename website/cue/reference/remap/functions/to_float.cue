package metadata

remap: functions: to_float: {
	category: "Coerce"
	description: """
		Coerces the `value` into a float.
		"""
	notices: ["Since [VRL v0.20.0](\(urls.vrl_changelog)#0200-2024-11-27) which is used since Vector v0.43.0, this function behaves differently than [parse_float](\(urls.vrl_functions)#parse_float). Notably, `to_float` will return an error for [non-normal](\(urls.is_normal)) floats whereas [parse_float](\(urls.vrl_functions)#parse_float) can return `inf` which might result in unsafe arithemtic operations that panic.  This will be addressed in the future."]
	arguments: [
		{
			name: "value"
			description: """
				The value to convert to a float. Must be convertible to a float, otherwise an error is raised.
				"""
			required: true
			type: ["integer", "float", "boolean", "string", "timestamp"]
		},
	]
	internal_failure_reasons: [
		"`value` is not a supported float representation.",
	]
	return: {
		types: ["float"]
		rules: [
			"If `value` is a float, it will be returned as-is.",
			"If `value` is an integer, it will be returned as as a float.",
			"If `value` is a string, it must be the string representation of an float or else an error is raised.",
			"If `value` is a boolean, `0.0` is returned for `false` and `1.0` is returned for `true`.",
			"If `value` is a timestamp, a [Unix timestamp](\(urls.unix_timestamp)) with fractional seconds is returned.",
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
		{
			title: "Coerce to a float (timestamp)"
			source: """
				to_float(t'2020-12-30T22:20:53.824727Z')
				"""
			return: 1609366853.824727
		},
	]
}
