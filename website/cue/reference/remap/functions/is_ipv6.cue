package metadata

remap: functions: is_ipv6: {
	category: "IP"
	description: """
		Check if the string is a valid IPv6 address or not.
		"""

	arguments: [
		{
			name:        "value"
			description: "The IP address to check"
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: []
	return: {
		types: ["boolean"]
		rules: [
			#"Returns `true` if `value` is a valid IPv6 address."#,
			#"Returns `false` if `value` is anything else."#,
		]
	}

	examples: [
		{
			title: "Valid IPv6 address"
			source: """
				is_ipv6("2001:0db8:85a3:0000:0000:8a2e:0370:7334")
				"""
			return: true
		},
		{
			title: "Valid IPv4 address"
			source: """
				is_ipv6("10.0.102.37")
				"""
			return: false
		},
		{
			title: "Arbitrary string"
			source: """
				is_ipv6("foobar")
				"""
			return: false
		},
	]
}
