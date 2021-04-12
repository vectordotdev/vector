package metadata

remap: functions: to_regex: {
	category: "Coerce"
	description: """
		Coerces the `value` into a regex.
		Warning: Creating regex is expensive! Too much use of this function can decrease the throughput of vector. 
		"""

	arguments: [
		{
			name:        "value"
			description: "The value to convert to a regex."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: [
		"`value` is not a string.",
	]
	return: {
		types: ["regex"]
		rules: [
			#"If `value` is string that contains a valid regex, returns the regex constructed with this string."#,
		]
	}

	examples: [
		{
			title: "Coerce to a regex"
			source: #"""
				to_regex("^foo$") ?? r''
				"""#
			return: "r'^foo$'"
		},
	]
}
