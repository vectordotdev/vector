package metadata

remap: functions: parse_syslog: {
	category:    "Parse"
	description: """
		Parses the `value` in [Syslog](\(urls.syslog)) format.
		"""
	notices: [
		"""
		The function makes a best effort to parse the various Syslog formats that exists out in the wild. This includes
		[RFC 6587](\(urls.syslog_6587)), [RFC 5424](\(urls.syslog_5424)), [RFC 3164](\(urls.syslog_3164)), and other
		common variations (such as the Nginx Syslog style).
		""",
		"""
			All values are returned as strings. We recommend manually coercing values to desired types as you see fit.
			""",
	]

	arguments: [
		{
			name:        "value"
			description: "The text containing the Syslog message to parse."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: [
		"`value` is not a properly formatted Syslog message.",
	]
	return: types: ["object"]

	examples: [
		{
			title: "Parse Syslog log (5424)"
			source: """
				parse_syslog!(
					s'<13>1 2020-03-13T20:45:38.119Z dynamicwireless.name non 2426 ID931 [exampleSDID@32473 iut="3" eventSource= "Application" eventID="1011"] Try to override the THX port, maybe it will reboot the neural interface!'
				)
				"""
			return: {
				severity:  "notice"
				facility:  "user"
				timestamp: "2020-03-13T20:45:38.119Z"
				hostname:  "dynamicwireless.name"
				appname:   "non"
				procid:    2426
				msgid:     "ID931"
				message:   "Try to override the THX port, maybe it will reboot the neural interface!"
				"exampleSDID@32473": {
					eventID:     "1011"
					eventSource: "Application"
					iut:         "3"
				}
				version: 1
			}
		},
	]
}
