package metadata

remap: functions: timestamp: {
	category: "Type"
	description: """
		Returns the `value` if it's a timestamp and errors otherwise. This enables the type checker to guarantee that
		the returned value is a timestamp and can be used in any function that expects one.
		"""

	arguments: [
		{
			name:        "value"
			description: "The value that you need to ensure is a timestamp."
			required:    true
			type: ["any"]
		},
	]
	internal_failure_reasons: [
		"`value` isn't a timestamp.",
	]
	return: {
		types: ["timestamp"]
		rules: [
			#"Returns the `value` if it's a timestamp."#,
			#"Raises an error if not a timestamp."#,
		]
	}
	examples: [
		{
			title: "Declare a timestamp type"
			input: log: timestamp: "2020-10-10T16:00:00Z"
			source: #"""
				ok, err = timestamp(.timestamp)
				"""#
			return: "function call error for \"timestamp\" at (10:31): expected \"timestamp\", got \"string\""
		},
	]
}
