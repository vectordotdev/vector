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
	return: ["string"]
	category:    "Coerce"
	description: """
		Converts a Syslog [facility code](\(urls.syslog_facility)) into its corresponding keyword,
		i.e. 0 into `\"kern\"`, 1 into `\"user\", etc.
		"""
	examples: [
		{
			title: "Convert Syslog facility to level"
			input: log: facility: "4"
			source: ".level = parse_syslog_facility(.facility)"
			output: input & {log: level: "auth"}
		},
		{
			title: "Error"
			input: log: facility: 27
			source: ".level = parse_syslog_facility(.facility)"
			raise:  "Failed to parse"
		},
	]
}
