package metadata

remap: functions: sha3: {
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
				- SHA3-224
				- SHA3-256
				- SHA3-384
				- SHA3-512
				"""#
			required: false
			default:  "SHA3-512"
			type: ["string"]
		},
	]
	return: ["string"]
	category: "hash"
	description: #"""
		Calculates a sha3 hash of a given string.
		"""#
	examples: [
		{
			title: "Success"
			input: {
				text: #"foo"#
			}
			source: #"""
				.hash = sha3(.text, variant = "SHA3-224")
				"""#
			output: {
				hash: "f4f6779e153c391bbd29c95e72b0708e39d9166c7cea51d1f10ef58a"
			}
		},
		{
			title: "Error"
			input: {
				text: #"foo"#
			}
			source: #"""
					.hash = sha3(.text, variant = "SHA-NONE")
				"""#
			output: {
				error: remap.errors.ArgumentError
			}
		},
	]
}
