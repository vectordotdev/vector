package metadata

remap: functions: ip_to_ipv6: {
	arguments: [
		{
			name:        "value"
			description: "The ip address to convert to IPv6."
			required:    true
			type: ["string"]
		},
	]
	return: ["string"]
	category: "IP"
	description: #"""
		Converts an address to an IPv6 address.
		If the parameter is already an IPv6 address it is passed through
		untouched. IPv4 addresses are converted to IPv4 mapped
		IPv6 addresses.
		"""#
	examples: [
		{
			title: "IPv4 to IPv6"
			input: log: v4: "192.168.10.32"
			source: #"""
				.v6 = ip_to_ipv6(.v4)
				"""#
			output: input & {log: v6: "::ffff:192.168.10.32"}
		},
	]
}
