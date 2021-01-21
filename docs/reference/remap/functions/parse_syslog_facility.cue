package metadata

remap: functions: parse_syslog_facility: {
	arguments: [
		{
			name:        "value"
			description: "The facility code."
			required:    true
			type: ["integer"]
		},
	]
	internal_failure_reasons: [
		"`value` is not a defined Syslog facility",
	]
	return: ["string"]
	category:    "Parse"
	description: """
		Converts a Syslog [facility code](\(urls.syslog_facility)) into its corresponding keyword,
		i.e. 0 into `\"kern\"`, 1 into `\"user\", etc.
		"""
	examples: [
		{
			title:  "Convert Syslog facility to level"
			source: ".level = parse_syslog_facility(4)"
			return: level: "auth"
		},
		{
			title:  "Error"
			source: ".level = parse_syslog_facility(27)"
			raises: runtime: "Failed to parse"
		},
	]
}
