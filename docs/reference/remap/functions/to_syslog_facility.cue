package metadata

remap: functions: to_syslog_facility: {
	category:    "Coerce"
	description: """
		Coerces the `value`, a Syslog [facility code](\(urls.syslog_facility)), into its corresponding
		Syslog keyword. i.e. 0 into `"kern"`, 1 into `"user"`, etc.
		"""

	arguments: [
		{
			name:        "value"
			description: "The facility code."
			required:    true
			type: ["integer"]
		},
	]
	internal_failure_reasons: [
		"`value` is not a valid Syslog [facility code](\(urls.syslog_facility)).",
	]
	return: types: ["string"]

	examples: [
		{
			title: "Coerce to a Syslog facility"
			source: """
				to_syslog_facility("4")
				"""
			return: "auth"
		},
	]
}
