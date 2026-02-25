package metadata

remap: functions: decrypt_ip: {
	category: "IP"
	description: """
		Decrypts an IP address that was previously encrypted, restoring the original IP address.

		Supported Modes:

		* AES128 - Decrypts an IP address that was scrambled using AES-128 encryption. Can transform between IPv4 and IPv6.
		* PFX (Prefix-preserving) - Decrypts an IP address that was encrypted with prefix-preserving mode, where network hierarchy was maintained.
		"""
	notices: [
		"""
			The `aes128` mode implements the `ipcrypt-deterministic` algorithm from the IPCrypt specification, while the `pfx` mode implements the `ipcrypt-pfx` algorithm. This function reverses the encryption performed by `encrypt_ip` - the same key and algorithm that were used for encryption must be used for decryption.
			""",
	]

	arguments: [
		{
			name:        "ip"
			description: "The encrypted IP address to decrypt (v4 or v6)."
			required:    true
			type: ["string"]
		},
		{
			name:        "key"
			description: "The decryption key in raw bytes (not encoded). Must be the same key that was used for encryption. For AES128 mode, the key must be exactly 16 bytes. For PFX mode, the key must be exactly 32 bytes."
			required:    true
			type: ["string"]
		},
		{
			name:        "mode"
			description: "The decryption mode to use. Must match the mode used for encryption: either `aes128` or `pfx`."
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
			title: "Decrypt IPv4 address with AES128"
			source: #"""
				decrypted_ip = decrypt_ip!("72b9:a747:f2e9:72af:76ca:5866:6dcf:c3b0", "sixteen byte key", "aes128")
				decrypted_ip
				"""#
			return: "192.168.1.1"
		},
		{
			title: "Decrypt IPv6 address with AES128"
			source: #"""
				decrypted_ip = decrypt_ip!("c0e6:eb35:6887:f554:4c65:8ace:17ca:6c6a", "sixteen byte key", "aes128")
				decrypted_ip
				"""#
			return: "2001:db8::1"
		},
		{
			title: "Decrypt IPv4 address with prefix-preserving mode"
			source: #"""
				decrypted_ip = decrypt_ip!("33.245.248.61", "thirty-two bytes key for pfx use", "pfx")
				decrypted_ip
				"""#
			return: "192.168.1.1"
		},
		{
			title: "Decrypt IPv6 address with prefix-preserving mode"
			source: #"""
				decrypted_ip = decrypt_ip!("88bd:d2bf:8865:8c4d:84b:44f6:6077:72c9", "thirty-two bytes key for ipv6pfx", "pfx")
				decrypted_ip
				"""#
			return: "2001:db8::1"
		},
		{
			title: "Round-trip encryption and decryption"
			source: #"""
				original_ip = "192.168.1.100"
				key = "sixteen byte key"

				encrypted = encrypt_ip!(original_ip, key, "aes128")
				decrypted = decrypt_ip!(encrypted, key, "aes128")

				decrypted == original_ip
				"""#
			return: true
		},
	]
}
