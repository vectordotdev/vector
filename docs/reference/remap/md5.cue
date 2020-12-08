package metadata

remap: functions: md5: {
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
		Calculates an md5 hash of a given string.
		"""#
	examples: [
		{
			title: "Success"
			input: {
				text: #"foo"#
			}
			source: #"""
				.hash = md5(.text)
				"""#
			output: {
				hash: "acbd18db4cc2f85cedef654fccc4a4d8"
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
