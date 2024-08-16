package metadata

remap: functions: decrypt: {
	category: "Cryptography"
	description: """
		Decrypts a string with a symmetric encryption algorithm.

		Supported Algorithms:

		* AES-256-CFB (key = 32 bytes, iv = 16 bytes)
		* AES-192-CFB (key = 24 bytes, iv = 16 bytes)
		* AES-128-CFB (key = 16 bytes, iv = 16 bytes)
		* AES-256-OFB (key = 32 bytes, iv = 16 bytes)
		* AES-192-OFB  (key = 24 bytes, iv = 16 bytes)
		* AES-128-OFB (key = 16 bytes, iv = 16 bytes)
		* Deprecated - AES-256-CTR (key = 32 bytes, iv = 16 bytes)
		* Deprecated - AES-192-CTR (key = 24 bytes, iv = 16 bytes)
		* Deprecated - AES-128-CTR (key = 16 bytes, iv = 16 bytes)
		* AES-256-CTR-LE (key = 32 bytes, iv = 16 bytes)
		* AES-192-CTR-LE (key = 24 bytes, iv = 16 bytes)
		* AES-128-CTR-LE (key = 16 bytes, iv = 16 bytes)
		* AES-256-CTR-BE (key = 32 bytes, iv = 16 bytes)
		* AES-192-CTR-BE (key = 24 bytes, iv = 16 bytes)
		* AES-128-CTR-BE (key = 16 bytes, iv = 16 bytes)
		* AES-256-CBC-PKCS7 (key = 32 bytes, iv = 16 bytes)
		* AES-192-CBC-PKCS7 (key = 24 bytes, iv = 16 bytes)
		* AES-128-CBC-PKCS7 (key = 16 bytes, iv = 16 bytes)
		* AES-256-CBC-ANSIX923 (key = 32 bytes, iv = 16 bytes)
		* AES-192-CBC-ANSIX923 (key = 24 bytes, iv = 16 bytes)
		* AES-128-CBC-ANSIX923 (key = 16 bytes, iv = 16 bytes)
		* AES-256-CBC-ISO7816 (key = 32 bytes, iv = 16 bytes)
		* AES-192-CBC-ISO7816 (key = 24 bytes, iv = 16 bytes)
		* AES-128-CBC-ISO7816 (key = 16 bytes, iv = 16 bytes)
		* AES-256-CBC-ISO10126 (key = 32 bytes, iv = 16 bytes)
		* AES-192-CBC-ISO10126 (key = 24 bytes, iv = 16 bytes)
		* AES-128-CBC-ISO10126 (key = 16 bytes, iv = 16 bytes)
		* CHACHA20-POLY1305 (key = 32 bytes, iv = 12 bytes)
		* XCHACHA20-POLY1305 (key = 32 bytes, iv = 24 bytes)
		* XSALSA20-POLY1305 (key = 32 bytes, iv = 24 bytes)
		"""

	arguments: [
		{
			name:        "ciphertext"
			description: "The string in raw bytes (not encoded) to decrypt."
			required:    true
			type: ["string"]
		},
		{
			name:        "algorithm"
			description: "The algorithm to use."
			required:    true
			type: ["string"]
		},
		{
			name:        "key"
			description: "The key in raw bytes (not encoded) for decryption. The length must match the algorithm requested."
			required:    true
			type: ["string"]
		},
		{
			name: "iv"
			description: #"""
				The IV in raw bytes (not encoded) for decryption. The length must match the algorithm requested.
				A new IV should be generated for every message. You can use `random_bytes` to generate a cryptographically secure random value.
				The value should match the one used during encryption.
				"""#
			required: true
			type: ["string"]
		},
	]
	internal_failure_reasons: [
		"`algorithm` is not a supported algorithm.",
		"`key` length does not match the key size required for the algorithm specified.",
		"`iv` length does not match the `iv` size required for the algorithm specified.",
	]
	return: types: ["string"]

	examples: [
		{
			title: "Decrypt value"
			source: #"""
				ciphertext = decode_base64!("5fLGcu1VHdzsPcGNDio7asLqE1P43QrVfPfmP4i4zOU=")
				iv = decode_base64!("fVEIRkIiczCRWNxaarsyxA==")
				key = "16_byte_keyxxxxx"
				decrypt!(ciphertext, "AES-128-CBC-PKCS7", key, iv: iv)
				"""#
			return: "super_secret_message"
		},
	]
}
