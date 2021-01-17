package metadata

remap: functions: ip_subnet: {
	arguments: [
		{
			name:        "ip"
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
	internal_failure_reasons: [
		"`ip` is not a valid IP address",
		"`subnet` is not a valid subnet.",
	]
	return: ["string"]
	category: "IP"
	description: #"""
		Extracts the subnet address from the given `ip` using the supplied `subnet`.

		Works with both IPv4 and IPv6 addresses. The IP version for the mask must be the same as the supplied address.
		"""#
	examples: [
		{
			title: "IPv4 subnet"
			input: log: address: "192.168.10.32"
			source: #"""
				.subnet = ip_subnet(.address, "255.255.255.0")
				"""#
			output: input & {log: subnet: "192.168.10.0"}
		},
		{
			title: "IPv6 subnet"
			input: log: address: "2404:6800:4003:c02::64"
			source: #"""
				.subnet = ip_subnet(.address, "/32")
				"""#
			output: input & {log: subnet: "2404:6800::"}
		},
	]
}
