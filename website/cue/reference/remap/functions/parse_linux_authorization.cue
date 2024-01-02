package metadata

remap: functions: parse_linux_authorization: {
	category:    "Parse"
	description: """
		Parses Linux authorization logs usually found under either `/var/log/auth.log` (for Debian-based systems) or
		`/var/log/secure` (for RedHat-based systems) according to [Syslog](\(urls.syslog)) format.
		"""
	notices: [
		"""
			The function resolves the year for messages that don't include it. If the current month is January, and the message is for
			December, it will take the previous year. Otherwise, take the current year.
			""",
	]

	arguments: [
		{
			name:        "value"
			description: "The text containing the message to parse."
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
			title: "Parse Linux authorization event"
			source: """
				parse_linux_authorization!(
					s'Mar 23 2023 01:49:58 localhost sshd[1111]: Accepted publickey for eng from 10.1.1.1 port 8888 ssh2: RSA SHA256:foobar'
				)
				"""
			return: {
				appname:   "sshd"
				hostname:  "localhost"
				message:   "Accepted publickey for eng from 10.1.1.1 port 8888 ssh2: RSA SHA256:foobar"
				procid:    1111
				timestamp: "2023-03-23T01:49:58Z"
			}
		},
	]
}
