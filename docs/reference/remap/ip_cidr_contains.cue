package metadata

remap: functions: ip_cidr_contains: {
	arguments: [
		{
			name:        "cidr"
			description: "The CIDR mask - either v4 or v6."
			required:    true
			type: ["string"]
		},
		{
			name:        "value"
			description: "The ip address - either a v4 or a v6 address."
			required:    true
			type: ["string"]
		},
	]
	return: ["boolean"]
	category: "IP"
	description: #"""
		Returns `true` if the given ip address is contained within the block referenced
		by the given CIDR mask.
		"""#
	examples: [
		{
			title: "IPv4 contains CIDR"
			input: log: address: "192.168.10.32"
			source: #"""
				.cidr = ip_cidr_contains(.address, "192.168.0.0/16")
				"""#
			output: input & {log: cidr: true}
		},
		{
			title: "IPv6 contains CIDR"
			input: log: address: "2001:4f8:3:ba:2e0:81ff:fe22:d1f1"
			source: #"""
				.cidr = ip_cidr_contains(.address, "2001:4f8:4:ba::/64")
				"""#
			output: input & {log: cidr: false}
		},
	]
}
