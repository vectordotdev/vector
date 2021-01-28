package metadata

remap: functions: to_syslog_level: {
	category:    "Coerce"
	description: """
		Coerces the `value`, a Syslog [severity level](\(urls.syslog_levels)), into its corresponding keyword,
		i.e. 0 into `"emerg"`, 1 into `"alert", etc.
		"""

	arguments: [
		{
			name:        "value"
			description: "The severity level."
			required:    true
			type: ["integer"]
		},
	]
	internal_failure_reasons: [
		"`value` is not a valid Syslog [severity level](\(urls.syslog_levels)).",
	]
	return: types: ["string"]

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
