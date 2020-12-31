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
				syslog_facility: "4"
			}
			source: ".log_facility = to_syslog_facility(.syslog_facility)"
			output: {
				log_facility: "auth"
			}
		},
		{
			title: "Error"
			input: {
				syslog_facility: "1337"
			}
			source: ".log_facility = to_syslog_facility(.syslog_facility)"
			output: {
				error: remap.errors.ParseError
			}
		},
	]
}
