package metadata

remap: functions: to_syslog_facility_code: {
	category:    "Convert"
	description: """
		Converts the `value`, a Syslog [facility keyword](\(urls.syslog_facility)), into a Syslog integer
		facility code (`0` to `23`).
		"""

	arguments: [
		{
			name:        "value"
			description: "The Syslog facility keyword to convert."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: [
		"`value` is not a valid Syslog facility keyword.",
	]
	return: types: ["integer"]

	examples: [
		{
			title: "Coerce to Syslog facility code"
			source: """
				to_syslog_facility_code!("authpriv")
				"""
			return: 10
		},
	]
}
