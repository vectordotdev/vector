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
	category: "networking"
	description: #"""
		Converts an address to an IPv6 address.
		If the parameter is already an IPv6 address it is passed through
		untouched. IPv4 addresses are converted to IPv4 mapped
		IPv6 addresses.
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
				v6:      "::ffff:192.168.10.32"
			}
		},
	]
}
