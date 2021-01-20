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
			title: "Coerce to a Syslog level"
			source: """
				to_syslog_level("5")
				"""
			return: "notice"
		},
	]
}
