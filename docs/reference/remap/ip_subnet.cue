package metadata

remap: functions: ip_subnet: {
	arguments: [
		{
			name:        "value"
			description: "The ip address - either a v4 or a v6 address."
			required:    true
			type: ["string"]
		},
		{
			name: "subnet"
			description: #"""
				The subnet to extract from the ip address. This can be either in the form of a prefix length,
				eg. `/8` or as a net mask - `255.255.0.0`. The net mask can be either an IPv4 or IPv6 address.
				"""#
			required: true
			type: ["string"]
		},
	]
	return: ["string"]
	category: "networking"
	description: #"""
		Extracts the subnet address from a given IP address using a supplied subnet mask or prefix length.
		Works with both IPv4 and IPv6 addresses. The IP version for the mask must be the same as the
		supplied address.
		"""#
	examples: [
		{
			title: "IPv4"
			input: {
				address: "192.168.10.32"
			}
			source: #"""
				.subnet = ip_subnet(.address, "255.255.255.0")
				"""#
			output: {
				address: "192.168.10.32"
				subnet:  "192.168.10.0"
			}
		},
		{
			title: "IPv6"
			input: {
				address: "2404:6800:4003:c02::64"
			}
			source: #"""
				.subnet = ip_subnet(.address, "/32")
				"""#
			output: {
				address: "2404:6800:4003:c02::64"
				subnet:  "2404:6800::"
			}
		},
	]
}
