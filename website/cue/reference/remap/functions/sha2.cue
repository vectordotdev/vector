package metadata

remap: functions: sha2: {
	category:    "Cryptography"
	description: """
		Calculates a [SHA-2](\(urls.sha2)) hash of the `value`.
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
				"SHA-224":     "SHA-224 algorithm"
				"SHA-256":     "SHA-256 algorithm"
				"SHA-384":     "SHA-384 algorithm"
				"SHA-512":     "SHA-512 algorithm"
				"SHA-512/224": "SHA-512/224 algorithm"
				"SHA-512/256": "SHA-512/256 algorithm"
			}
			required: false
			default:  "SHA-512/256"
			type: ["string"]
		},
	]
	internal_failure_reasons: []
	return: types: ["string"]

	examples: [
		{
			title: "Calculate sha2 hash"
			source: #"""
				sha2("foo", variant: "SHA-512/224")
				"""#
			return: "d68f258d37d670cfc1ec1001a0394784233f88f056994f9a7e5e99be"
		},
	]
}
