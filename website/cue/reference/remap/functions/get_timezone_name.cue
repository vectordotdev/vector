package metadata

remap: functions: get_timezone_name: {
	category:    "System"
	description: """
		Returns the name of the timezone in the Vector configuration (see
		[global configuration options](\(urls.vector_configuration_global))).
		If the configuration is set to `local`, then it attempts to
		determine the name of the timezone from the host OS. If this
		is not possible, then it returns the fixed offset of the
		local timezone for the current time in the format `"[+-]HH:MM"`,
		for example, `"+02:00"`.
		"""

	arguments: []
	internal_failure_reasons: [
		"Retrieval of local timezone information failed.",
	]
	return: types: ["string"]

	examples: [
		{
			title: "Get Vector's timezone when the timezone config is set to 'America/Chicago'"
			input: log: {}
			source: #"""
				.vector_timezone = get_timezone_name!()
				"""#
			output: log: vector_timezone: "America/Chicago"
		},
		{
			title: "Get Vector's timezone when the timezone config is set to 'local' and the host OS has a local timezone of 'America/New_York'"
			input: log: {}
			source: #"""
				.vector_timezone = get_timezone_name!()
				"""#
			output: log: vector_timezone: "America/New_York"
		},
		{
			title: "Get Vector's timezone when the timezone config is set to 'local' and the host OS can only determine the local timezone offset"
			input: log: {}
			source: #"""
				.vector_timezone = get_timezone_name!()
				"""#
			output: log: vector_timezone: "-05:00"
		},
	]
}
