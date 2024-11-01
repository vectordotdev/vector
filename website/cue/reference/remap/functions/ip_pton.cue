package metadata

remap: functions: ip_pton: {
	category:    "IP"
	description: """
		Converts IPv4 and IPv6 addresses from text to binary form.

		* The binary form of IPv4 addresses is 4 bytes (32 bits) long.
		* The binary form of IPv6 addresses is 16 bytes (128 bits) long.

		This behavior mimics [inet_pton](\(urls.ip_pton)).
		"""

	notices: [
		"""
			The binary data from this function is not easily printable.
			However, functions such as `encode_base64` or `encode_percent` can
			still process it correctly.
			""",
	]

	arguments: [
		{
			name:        "value"
			description: "The IP address (v4 or v6) to convert to binary form."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: [
		"`value` is not a valid IP (v4 or v6) address in text form.",
	]
	return: types: ["string"]

	examples: [
		{
			title: "Convert IPv4 address to bytes and encode to Base64"
			source: #"""
				encode_base64(ip_pton!("192.168.0.1"))
				"""#
			return: "wKgAAQ=="
		},
		{
			title: "Convert IPv6 address to bytes and encode to Base64"
			source: #"""
				encode_base64(ip_pton!("2001:db8:85a3::8a2e:370:7334"))
				"""#
			return: "IAENuIWjAAAAAIouA3BzNA=="
		},
	]
}
