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
				hmac("Hello there", "supersecretkey")
				"""#
			return: "kmpc79vrb6SODvg4LwivUnb443+IhR9SSW55KcBPKo8="
		},
		{
			title: "Calculate message HMAC (SHA-224, hex-encoded result)"
			source: #"""
				hmac("Hello there", "supersecretkey", algorithm: "SHA-224", encoding: "hex")
				"""#
			return: "5e3204bc7ac3212178db2ccbe715d3714482dd6f625de19d19682380"
		},
	]
}
