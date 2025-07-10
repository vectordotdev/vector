package metadata

remap: functions: hmac: {
	category:    "Cryptography"
	description: """
		Calculates a [HMAC](\(urls.hmac)) of the `value` using the given `key`.
		The hashing `algorithm` used can be optionally specified.

		For most use cases, the resulting bytestream should be encoded into a hex or base64
		string using either [encode_base16](\(urls.vrl_functions)/#encode_base16) or
		[encode_base64](\(urls.vrl_functions)/#encode_base64).

		This function is infallible if either the default `algorithm` value or a recognized-valid compile-time
		`algorithm` string literal is used. Otherwise, it is fallible.
		"""

	arguments: [
		{
			name:        "value"
			description: "The string to calculate the HMAC for."
			required:    true
			type: ["string"]
		},
		{
			name:        "key"
			description: "The string to use as the cryptographic key."
			required:    true
			type: ["string"]
		},
		{
			name:        "algorithm"
			description: "The hashing algorithm to use."
			enum: {
				"SHA1":    "SHA1 algorithm"
				"SHA-224": "SHA-224 algorithm"
				"SHA-256": "SHA-256 algorithm"
				"SHA-384": "SHA-384 algorithm"
				"SHA-512": "SHA-512 algorithm"
			}
			required: false
			default:  "SHA-256"
			type: ["string"]
		},
	]
	internal_failure_reasons: []
	return: types: ["string"]

	examples: [
		{
			title: "Calculate message HMAC (defaults: SHA-256), encoding to a base64 string"
			source: #"""
				encode_base64(hmac("Hello there", "super-secret-key"))
				"""#
			return: "eLGE8YMviv85NPXgISRUZxstBNSU47JQdcXkUWcClmI="
		},
		{
			title: "Calculate message HMAC using SHA-224, encoding to a hex-encoded string"
			source: #"""
				encode_base16(hmac("Hello there", "super-secret-key", algorithm: "SHA-224"))
				"""#
			return: "42fccbc2b7d22a143b92f265a8046187558a94d11ddbb30622207e90"
		},
		{
			title: "Calculate message HMAC using a variable hash algorithm"
			source: #"""
				.hash_algo = "SHA-256"
				hmac_bytes, err = hmac("Hello there", "super-secret-key", algorithm: .hash_algo)
				if err == null {
					.hmac = encode_base16(hmac_bytes)
				}
				"""#
			return: "78b184f1832f8aff3934f5e0212454671b2d04d494e3b25075c5e45167029662"
		},
	]
}
