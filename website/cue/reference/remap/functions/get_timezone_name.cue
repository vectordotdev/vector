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
			title: "Get the IANA name of Vector's timezone"
			input: log: {}
			source: #"""
				.vector_timezone = get_timezone_name!()
				"""#
			output: log: vector_timezone: "UTC"
		},
	]
}
