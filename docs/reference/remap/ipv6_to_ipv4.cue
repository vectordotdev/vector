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
	category: "networking"
	description: #"""
		Converts an IPv4 mapped IPv6 address to an IPv4 address.
		This function will raise an error if the input address is not a compatible address.
		"""#
	examples: [
		{
			title: "Success"
			input: {
				address: "::ffff:192.168.0.1"
			}
			source: #"""
				.v4 = ipv6_to_ipv4(.address)
				"""#
			output: {
				address: "::ffff:192.168.0.1"
				v4:      "192.168.0.1"
			}
		},
		{
			title: "Error"
			input: {
				address: "2001:0db8:85a3::8a2e:0370:7334"
			}
			source: #"""
				.v4 = ipv6_to_ipv4(.address)
				"""#
			output: {
				error: "function call error: IPV6 address 2001:db8:85a3::8a2e:370:7334 is not compatible with IPV4"
			}
		},

	]
}
