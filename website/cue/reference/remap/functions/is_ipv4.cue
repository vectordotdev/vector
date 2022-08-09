package metadata

remap: functions: is_ipv4: {
	category: "IP"
	description: """
		Check if the string is a valid IPv4 address or not.

		An [IPv4-mapped][https://datatracker.ietf.org/doc/html/rfc6890] or
		[IPv4-compatible][https://datatracker.ietf.org/doc/html/rfc6890] IPv6 address is not considered
		valid for the purpose of this function.
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
			#"Returns `true` if `value` is a valid IPv4 address."#,
			#"Returns `false` if `value` is anything else."#,
		]
	}

	examples: [
		{
			title: "Valid IPv4 address"
			source: """
				is_ipv4("10.0.102.37")
				"""
			return: true
		},
		{
			title: "Valid IPv6 address"
			source: """
				is_ipv4("2001:0db8:85a3:0000:0000:8a2e:0370:7334")
				"""
			return: false
		},
		{
			title: "Arbitrary string"
			source: """
				is_ipv4("foobar")
				"""
			return: false
		},
	]
}
