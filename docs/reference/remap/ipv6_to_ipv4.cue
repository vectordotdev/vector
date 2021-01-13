package metadata

remap: functions: ipv6_to_ipv4: {
	arguments: [
		{
			name:        "ip"
			description: "The IPv4 mapped IPv6 address to convert."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: [
		"`ip` is not a valid IP address",
	]
	return: ["string"]
	category: "IP"
	description: #"""
		Converts the provided `ip` to an IPv4 address.
		"""#
	examples: [
		{
			title: "IPv6 to IPv4"
			input: log: v6: "::ffff:192.168.0.1"
			source: #"""
				.v4 = ipv6_to_ipv4(.address)
				"""#
			output: input & {log: v4: "192.168.0.1"}
		},
	]
}
