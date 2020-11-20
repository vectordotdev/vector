package metadata

remap: functions: ip_to_ipv6: {
	arguments: [
		{
			name:        "value"
			description: "The IPv4 ip address to convert."
			required:    true
			type: ["string"]
		},
	]
	return: ["string"]
	category: "ip"
	description: #"""
		Converts an IPv4 address to an IPv4 mapped IPv6 address.
		"""#
	examples: [
		{
			title: "IPv4"
			input: {
				address: "192.168.10.32"
			}
			source: #"""
				.v6 = ip_to_ipv6(.address)
				"""#
			output: {
				address: "192.168.10.32"
				v4:      "::ffff:192.168.10.32"
			}
		},
	]
}
