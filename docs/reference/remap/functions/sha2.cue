package metadata

remap: functions: sha2: {
	arguments: [
		{
			name:        "value"
			description: "The string to calculate the hash for."
			required:    true
			type: ["string"]
		},
		{
			name: "variant"
			description: #"""
				The variant of the algorithm to use.
				The allowed variants are:
				- SHA-224
				- SHA-256
				- SHA-384
				- SHA-512
				- SHA-512/224
				- SHA-512/256
				"""#
			required: false
			default:  "SHA-512/256"
			type: ["string"]
		},
	]
	internal_failure_reasons: []
	return: ["string"]
	category: "Hash"
	description: #"""
		Calculates a sha2 hash of the provided `value`.
		"""#
	examples: [
		{
			title: "Calculate sha2 hash"
			source: #"""
				sha2("foo, variant = "SHA-512/224")
				"""#
			return: "d68f258d37d670cfc1ec1001a0394784233f88f056994f9a7e5e99be"
		},
	]
}
