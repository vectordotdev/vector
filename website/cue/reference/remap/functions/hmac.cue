package metadata

remap: functions: hmac: {
	category:    "Cryptography"
	description: """
		Calculates a [HMAC](\(urls.hmac)) of the `value` using the given `key`.
		Both the hashing `algorithm` and the `encoding` format for the byte-string result can be optionally specified.
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
		{
			name:        "encoding"
			description: "The byte-string encoding to use for the result."
			enum: {
				"base64":  "Base64 encoding"
				"hex":     "Hex string encoding"
			}
			required: false
			default:  "base64"
			type: ["string"]
		}
	]
	internal_failure_reasons: []
	return: types: ["string"]

	examples: [
		{
			title: "Calculate message HMAC (defaults: SHA-256, base64-encoded result)"
			source: #"""
				hmac("Hello there", "super-secret-key")
				"""#
			return: "eLGE8YMviv85NPXgISRUZxstBNSU47JQdcXkUWcClmI="
		},
		{
			title: "Calculate message HMAC (SHA-224, hex-encoded result)"
			source: #"""
				hmac("Hello there", "super-secret-key", algorithm: "SHA-224", encoding: "hex")
				"""#
			return: "42fccbc2b7d22a143b92f265a8046187558a94d11ddbb30622207e90"
		},
	]
}
