package metadata

remap: functions: timestamp: {
	category: "Type"
	description: """
		Errors if `value` is not a timestamp, if `value` is a timestamp it is returned.

		This allows the type checker to guarantee that the returned value is a timestamp and can be used in any function
		that expects this type.
		"""

	arguments: [
		{
			name:        "value"
			description: "The value to ensure is a timestamp."
			required:    true
			type: ["any"]
		},
	]
	internal_failure_reasons: [
		"`value` is not a timestamp.",
	]
	return: {
		types: ["string"]
		rules: [
			#"If `value` is a timestamp then it is returned."#,
			#"Otherwise an error is raised."#,
		]
	}
	examples: [
		{
			title: "Declare a timestamp type"
			input: log: timestamp: "2020-10-10T16:00:00Z"
			source: #"""
				timestamp(.timestamp)
				"""#
			return: input.log.timestamp
		},
	]
}
