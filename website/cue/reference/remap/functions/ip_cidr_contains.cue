package metadata

remap: functions: ip_cidr_contains: {
	category: "IP"
	description: """
		Determines whether the `ip` is contained in the block referenced by the `cidr`.
		"""

	arguments: [
		{
			name:        "cidr"
			description: "The CIDR mask (v4 or v6)."
			required:    true
			type: ["string"]
		},
		{
			name:        "ip"
			description: "The IP address (v4 or v6)."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: [
		"`cidr` is not a valid CIDR.",
		"`ip` is not a valid IP address.",
	]
	return: types: ["boolean"]

	examples: [
		{
			title: "IPv4 contains CIDR"
			source: #"""
				ip_cidr_contains!("192.168.0.0/16", "192.168.10.32")
				"""#
			return: true
		},
		{
			title: "IPv6 contains CIDR"
			source: #"""
				ip_cidr_contains!("2001:4f8:4:ba::/64", "2001:4f8:4:ba:2e0:81ff:fe22:d1f1")
				"""#
			return: true
		},
	]
}
