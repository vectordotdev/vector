package metadata

remap: functions: to_level: {
	arguments: [
		{
			name:        "severity"
			description: "The integer severity level."
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
			source: ".log_level = to_level(.severity)"
			output: {
				level: "notice"
			}
		},
		{
			title: "Error"
			input: {
				severity: "1337"
			}
			source: ".log_level = to_severity(.severity)"
			output: {
				error: remap.errors.ParseError
			}
		},
	]
}
