package metadata

remap: functions: ip_ntop: {
	category:    "IP"
	description: """
		Converts IPv4 and IPv6 addresses from binary to text form.

		This behavior mimics [inet_ntop](\(urls.ip_ntop)).
		"""

	notices: [
		"""
			The binary data for this function is not easily printable.
			However, the results from functions such as `decode_base64` or
			`decode_percent` can still be used correctly.
			""",
	]

	arguments: [
		{
			name: "value"
			description: """
				The binary data to convert from.
				For IPv4 addresses, it must be 4 bytes (32 bits) long.
				For IPv6 addresses, it must be 16 bytes (128 bits) long.
				"""
			required: true
			type: ["string"]
		},
	]
	internal_failure_reasons: [
		"`value` must be of length 4 or 16 bytes.",
	]
	return: types: ["string"]

	examples: [
		{
			title: "Convert IPv4 address from bytes after decoding from Base64"
			source: #"""
				ip_ntop!(decode_base64!("wKgAAQ=="))
				"""#
			return: "192.168.0.1"
		},
		{
			title: "Convert IPv6 address from bytes after decoding from Base64"
			source: #"""
				ip_ntop!(decode_base64!("IAENuIWjAAAAAIouA3BzNA=="))
				"""#
			return: "2001:db8:85a3::8a2e:370:7334"
		},
	]
}
