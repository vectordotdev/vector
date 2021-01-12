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
	internal_failure_reasons: [
		"`value` is not a valid Syslog level",
	]
	return: ["string"]
	category:    "Coerce"
	description: """
		Coerces the provided `value`, a Syslog [severity level](\(urls.syslog_levels)), into its corresponding keyword,
		i.e. 0 into `"emerg"`, 1 into `"alert", etc.
		"""
	examples: [
		{
			title: "Convert Syslog severity to level"
			input: log: severity: "5"
			source: ".log_level = to_syslog_level(.severity)"
			output: input & {log: level: "notice"}
		},
		{
			title: "Error"
			input: log: severity: "1337"
			source: ".log_level = to_syslog_level(.severity)"
			raise:  "Failed to parse"
		},
	]
}
