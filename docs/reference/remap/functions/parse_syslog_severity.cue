package metadata

remap: functions: parse_syslog_severity: {
	arguments: [
		{
			name:        "value"
			description: "The Syslog level keyword to convert."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: [
		"`value` is not a defined Syslog level",
	]
	return: types: ["integer"]
	category:    "Parse"
	description: """
		Converts a Syslog [log level keyword](\(urls.syslog_levels)) into an integer severity level
		(0 to 7). Throws an error if the level isn't recognized. The now-deprecated keywords
		`panic`, `error`, and `warn` are converted to `0`, `3`, and `4` respectively.
		"""

	examples: [
		{
			title:  "Concert Syslog level to severity"
			source: #".severity = parse_syslog_severity("alert")"#
			return: severity: 1
		},
	]
}
