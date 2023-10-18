package metadata

remap: functions: ip_aton: {
	category:    "IP"
	description: """
		Converts IPv4 address in numbers-and-dots notation into network-order
		bytes represented as an integer.

		This behavior mimics [inet_aton](\(urls.ip_aton)).
		"""

	arguments: [
		{
			name:        "value"
			description: "The IP address to convert to binary."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: [
		"`value` is not a valid IPv4 address.",
	]
	return: types: ["integer"]

	examples: [
		{
			title: "IPv4 to integer"
			source: #"""
				ip_aton!("1.2.3.4")
				"""#
			return: 16909060
		},
	]
}
