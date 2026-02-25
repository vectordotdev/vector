package metadata

remap: functions: encrypt_ip: {
	category: "IP"
	description: """
		Encrypts an IP address, transforming it into a different valid IP address.

		Supported Modes:

		* AES128 - Scrambles the entire IP address using AES-128 encryption. Can transform between IPv4 and IPv6.
		* PFX (Prefix-preserving) - Maintains network hierarchy by ensuring that IP addresses within the same network are encrypted to addresses that also share a common network. This preserves prefix relationships while providing confidentiality.
		"""
	notices: [
		"""
			The `aes128` mode implements the `ipcrypt-deterministic` algorithm from the IPCrypt specification, while the `pfx` mode implements the `ipcrypt-pfx` algorithm. Both modes provide deterministic encryption where the same input IP address encrypted with the same key will always produce the same encrypted output.
			""",
	]

	arguments: [
		{
			name:        "ip"
			description: "The IP address to encrypt (v4 or v6)."
			required:    true
			type: ["string"]
		},
		{
			name:        "key"
			description: "The encryption key in raw bytes (not encoded). For AES128 mode, the key must be exactly 16 bytes. For PFX mode, the key must be exactly 32 bytes."
			required:    true
			type: ["string"]
		},
		{
			name:        "mode"
			description: "The encryption mode to use. Must be either `aes128` or `pfx`."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: [
		"`ip` is not a valid IP address.",
		"`mode` is not a supported mode (must be `aes128` or `pfx`).",
		"`key` length does not match the requirements for the specified mode (16 bytes for `aes128`, 32 bytes for `pfx`).",
	]
	return: types: ["string"]

	examples: [
		{
			title: "Encrypt IPv4 address with AES128"
			source: #"""
				encrypted_ip = encrypt_ip!("192.168.1.1", "sixteen byte key", "aes128")
				encrypted_ip
				"""#
			return: "72b9:a747:f2e9:72af:76ca:5866:6dcf:c3b0"
		},
		{
			title: "Encrypt IPv6 address with AES128"
			source: #"""
				encrypted_ip = encrypt_ip!("2001:db8::1", "sixteen byte key", "aes128")
				encrypted_ip
				"""#
			return: "c0e6:eb35:6887:f554:4c65:8ace:17ca:6c6a"
		},
		{
			title: "Encrypt IPv4 address with prefix-preserving mode"
			source: #"""
				encrypted_ip = encrypt_ip!("192.168.1.1", "thirty-two bytes key for pfx use", "pfx")
				encrypted_ip
				"""#
			return: "33.245.248.61"
		},
		{
			title: "Encrypt IPv6 address with prefix-preserving mode"
			source: #"""
				encrypted_ip = encrypt_ip!("2001:db8::1", "thirty-two bytes key for ipv6pfx", "pfx")
				encrypted_ip
				"""#
			return: "88bd:d2bf:8865:8c4d:84b:44f6:6077:72c9"
		},
	]
}
