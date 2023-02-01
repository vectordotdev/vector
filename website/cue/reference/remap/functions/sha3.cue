package metadata

remap: functions: sha3: {
	category:    "Cryptography"
	description: """
		Calculates a [SHA-3](\(urls.sha3)) hash of the `value`.
		"""

	arguments: [
		{
			name:        "value"
			description: "The string to calculate the hash for."
			required:    true
			type: ["string"]
		},
		{
			name:        "variant"
			description: "The variant of the algorithm to use."
			enum: {
				"SHA3-224": "SHA3-224 algorithm"
				"SHA3-256": "SHA3-256 algorithm"
				"SHA3-384": "SHA3-384 algorithm"
				"SHA3-512": "SHA3-512 algorithm"
			}
			required: false
			default:  "SHA3-512"
			type: ["string"]
		},
	]
	internal_failure_reasons: []
	return: types: ["string"]

	examples: [
		{
			title: "Calculate sha3 hash"
			source: #"""
				sha3("foo", variant: "SHA3-224")
				"""#
			return: "f4f6779e153c391bbd29c95e72b0708e39d9166c7cea51d1f10ef58a"
		},
	]
}
