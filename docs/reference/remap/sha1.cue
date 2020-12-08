package metadata

remap: functions: sha1: {
	arguments: [
		{
			name:        "value"
			description: "The string to calculate the hash for."
			required:    true
			type: ["string"]
		},
	]
	return: ["string"]
	category: "hash"
	description: #"""
		Calculates a sha1 hash of a given string.
		"""#
	examples: [
		{
			title: "Success"
			input: {
				text: #"foo"#
			}
			source: #"""
				.hash = sha1(.text)
				"""#
			output: {
				hash: "0beec7b5ea3f0fdbc95d0dd47f3c5bc275da8a33"
			}
		},
		{
			title: "Error"
			input: {
				text: 42
			}
			source: #"""
					.hash = sha1(.text)
				"""#
			output: {
				error: remap.errors.ArgumentError
			}
		},
	]
}
