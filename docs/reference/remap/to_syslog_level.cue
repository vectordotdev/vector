package metadata

remap: functions: to_syslog_level: {
	arguments: [
		{
			name:        "value"
			description: "The severity level."
			required:    true
			type: ["integer"]
		},
	]
	return: ["string"]
	category:    "parse"
	description: """
		Converts a Syslog [severity level](\(urls.syslog_levels)) into its corresponding keyword,
		i.e. 0 into `"emerg"`, 1 into `"alert", etc.
		"""
	examples: [
		{
			title: "Success"
			input: {
				severity: "5"
			}
			source: ".log_level = to_syslog_level(.severity)"
			output: {
				level: "notice"
			}
		},
		{
			title: "Error"
			input: {
				severity: "1337"
			}
			source: ".log_level = to_syslog_level(.severity)"
			output: {
				error: remap.errors.ParseError
			}
		},
	]
}
