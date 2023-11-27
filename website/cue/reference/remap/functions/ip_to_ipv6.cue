package metadata

remap: functions: ip_to_ipv6: {
	category: "IP"
	description: """
		Converts the `ip` to an IPv6 address.
		"""

	arguments: [
		{
			name:        "ip"
			description: "The IP address to convert to IPv6."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: [
		"`ip` is not a valid IP address.",
	]
	return: {
		types: ["string"]
		rules: [
			"The `ip` is returned unchanged if it's already an IPv6 address.",
			"The `ip` is converted to an IPv6 address if it's an IPv4 address.",
		]
	}

	examples: [
		{
			title: "IPv4 to IPv6"
			source: #"""
				ip_to_ipv6!("192.168.10.32")
				"""#
			return: "::ffff:192.168.10.32"
		},
	]
}
