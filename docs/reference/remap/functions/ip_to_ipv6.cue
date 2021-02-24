package metadata

remap: functions: ip_to_ipv6: {
	category: "IP"
	description: """
		Converts the `ip` to an IPv6 address.
		"""

	arguments: [
		{
			name:        "ip"
			description: "The ip address to convert to IPv6."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: [
		"`ip` is not a valid IP address",
	]
	return: {
		types: ["string"]
		rules: [
			"If `ip` is already an IPv6 address it is passed through untouched.",
			"If `ip` is a IPv4 address then it converted to IPv4 mapped IPv6 addresses.",
		]
	}

	examples: [
		{
			title: "IPv4 to IPv6"
			source: #"""
				ip_to_ipv6("192.168.10.32")
				"""#
			return: "::ffff:192.168.10.32"
		},
	]
}
