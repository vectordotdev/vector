package metadata

remap: functions: ip_subnet: {
	category: "IP"
	description: """
		Extracts the subnet address from the `ip` using the supplied `subnet`.
		"""
	notices: [
		"""
			Works with both IPv4 and IPv6 addresses. The IP version for the mask must be the same as the supplied
			address.
			""",
	]

	arguments: [
		{
			name:        "ip"
			description: "The IP address (v4 or v6)."
			required:    true
			type: ["string"]
		},
		{
			name: "subnet"
			description: #"""
				The subnet to extract from the IP address. This can be either a prefix length like `/8` or a net mask
				like `255.255.0.0`. The net mask can be either an IPv4 or IPv6 address.
				"""#
			required: true
			type: ["string"]
		},
	]
	internal_failure_reasons: [
		"`ip` is not a valid IP address.",
		"`subnet` is not a valid subnet.",
	]
	return: types: ["string"]

	examples: [
		{
			title: "IPv4 subnet"
			source: #"""
				ip_subnet!("192.168.10.32", "255.255.255.0")
				"""#
			return: "192.168.10.0"
		},
		{
			title: "IPv6 subnet"
			source: #"""
				ip_subnet!("2404:6800:4003:c02::64", "/32")
				"""#
			return: "2404:6800::"
		},
	]
}
