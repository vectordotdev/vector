package metadata

remap: functions: get_vector_timezone: {
	category: "System"
	description: """
		Returns the name of the timezone in the vector configuration
		(see [global configuration options](\(urls.vector_configuration_global))).
		If the configuration is set to `local`, then it attempts to
		determine from the host OS the name of the timezone. If this
		is not possible, then it will return the fixed offset of the
		local timezone for the current time in the format `"[+-]HH:MM"`,
		for example, `"+02:00"`
		"""

	arguments: []
	internal_failure_reasons: [
		"Retrieval of local timezone information failed.",
	]
	return: types: ["string"]

	examples: [
		{
			title: "Get vector timezone when timezone config is set to 'America/Chicago'"
			input: log: {}
			source: #"""
				.vector_timezone = get_vector_timezone!()
				"""#
			output: log: vector_timezone: "America/Chicago"
		},
		{
			title: "Get vector timezone when timezone config is set to 'local' and the host OS has a local timezone of America/New_York"
			input: log: {}
			source: #"""
				.vector_timezone = get_vector_timezone!()
				"""#
			output: log: vector_timezone: "America/New_York"
		},
		{
			title: "Get vector timezone when timezone config is set to 'local' and the host OS can only determine the local timezone offset"
			input: log: {}
			source: #"""
				.vector_timezone = get_vector_timezone!()
				"""#
			output: log: vector_timezone: "-05:00"
		},
	]
}
