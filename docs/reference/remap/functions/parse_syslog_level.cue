package metadata

remap: functions: parse_syslog_level: {
	arguments: [
		{
			name:        "value"
			description: "The severity level."
			required:    true
			type: ["integer"]
		},
	]
	internal_failure_reasons: [
		"`value` is not a defined Syslog severity",
	]
	return: ["string"]
	category:    "Parse"
	description: """
		Converts a Syslog [severity level](\(urls.syslog_levels)) into its corresponding keyword,
		i.e. 0 into `"emerg"`, 1 into `"alert", etc.
		"""
	examples: [
		{
			title:  "Convert Syslog severity to level"
			source: ".level = parse_syslog_level(5)"
			return: level: "notice"
		},
		{
			title:  "Error"
			source: ".level = parse_syslog_level(1337)"
			raises: runtime: "Failed to parse"
		},
	]
}
