package metadata

remap: functions: ipv6_to_ipv4: {
	arguments: [
		{
			name:        "value"
			description: "The IPv4 mapped IPv6 address to convert."
			required:    true
			type: ["string"]
		},
	]
	return: ["string"]
	category: "IP"
	description: #"""
		Converts an IPv4 mapped IPv6 address to an IPv4 address.
		This function will raise an error if the input address is not a compatible address.
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
