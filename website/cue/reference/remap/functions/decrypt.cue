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
		* AES-256-CTR (key = 32 bytes, iv = 16 bytes)
		* AES-192-CTR (key = 24 bytes, iv = 16 bytes)
		* AES-128-CTR (key = 16 bytes, iv = 16 bytes)
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
		"""

	arguments: [
		{
			name:        "ciphertext"
			description: "The string to decrypt. The should be raw bytes (not encoded)."
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
			description: "The key for decryption. The should be raw bytes of the key (not encoded). The length must match the algorithm requested."
			required:    true
			type: ["string"]
		},
		{
			name: "iv"
			description: #"""
				The IV for decryption. The should be raw bytes of the IV (not encoded). The length must match the algorithm requested.
				A new IV should be generated for every message. You can use `random_bytes` to generate a cryptographically secure random value.
				The value should match the one used during encryption.
				"""#
			required: true
			type: ["string"]
		},
	]
	internal_failure_reasons: [
		"`algorithm` isn't a supported algorithm",
		"`key` length doesn't match the key size required for the algorithm specified",
		"`iv` length doesn't match the iv size required for the algorithm specified",
	]
	return: types: ["string"]

	examples: [
		{
			title: "Decrypt value"
			source: #"""
				ciphertext = decode_base64!("5fLGcu1VHdzsPcGNDio7asLqE1P43QrVfPfmP4i4zOU=");
				iv = decode_base64!("fVEIRkIiczCRWNxaarsyxA==");
				key = "16_byte_keyxxxxx";
				decrypt!(ciphertext, "AES-128-CBC-PKCS7", key, iv: iv)
				"""#
			return: "super_secret_message"
		},
	]
}
