remap: functions: parse_syslog: {
	arguments: [
		{
			name:        "value"
			description: "The text containing the syslog message to parse."
			required:    true
			type: ["string"]
		},
	]
	return: ["map"]
	category: "Parse"
	description: #"""
		Parses a syslog message. The function makes a best effort to parse the various Syslog formats out in the wild.
		This includes [RFC 6587][urls.syslog_6587], [RFC 5424][urls.syslog_5424], [RFC 3164][urls.syslog_3164], and other
		common variations (such as the Nginx Syslog style). If parsing fails, Vector will include the entire Syslog
		line in the message field.
		"""#
	examples: [
		{
			title: "Parse Syslog meessage (success)"
			input: log: message: """
				<13>1 2020-03-13T20:45:38.119Z dynamicwireless.name non 2426 ID931 [exampleSDID@32473 iut="3" eventSource= "Application" eventID="1011"] Try to override the THX port, maybe it will reboot the neural interface!
				"""
			source: ". = parse_syslog(del(.message))"
			output: log: {
				severity:    "notice"
				facility:    "user"
				timestamp:   "2020-03-13T20:45:38.119Z"
				hostname:    "dynamicwireless.name"
				appname:     "non"
				procid:      "2426"
				msgid:       "ID931"
				iut:         "3"
				eventSource: "Application"
				eventID:     "1011"
				message:     "Try to override the THX port, maybe it will reboot the neural interface!"
			}
		},
		{
			title: "Parse Syslog meessage (error)"
			input: log: message: "I am not a Syslog message"
			source: ". = parse_syslog(del(.message))"
			raise:  "Failed to parse"
		},
	]
}
