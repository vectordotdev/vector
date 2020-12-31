package metadata

remap: functions: to_syslog_facility: {
	arguments: [
		{
			name:        "value"
			description: "The facility code."
			required:    true
			type: ["integer"]
		},
	]
	return: ["string"]
	category:    "Coerce"
	description: """
		Converts a Syslog [facility code](\(urls.syslog_facility)) into its corresponding keyword,
		i.e. 0 into `"kern"`, 1 into `"user", etc.
		"""
	examples: [
		{
			title: "Success"
			input: {
				SYSLOG_FACILITY: "4"
			}
			source: ".log_facility = to_syslog_facility(.SYSLOG_FACILITY)"
			output: {
				log_facility: "auth"
			}
		},
		{
			title: "Error"
			input: {
				SYSLOG_FACILITY: "1337"
			}
			source: ".log_facility = to_syslog_facility(.SYSLOG_FACILITY)"
			output: {
				error: remap.errors.ParseError
			}
		},
	]
}
