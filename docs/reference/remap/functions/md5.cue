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
	internal_failure_reasons: []
	return: ["string"]
	category: "Hash"
	description: #"""
		Calculates an md5 hash of a given `value`.
		"""#
	examples: [
		{
			title: "Create md5 hash"
			input: log: text: #"foo"#
			source: #"""
				.hash = md5(.text)
				"""#
			output: input & {log: hash: "acbd18db4cc2f85cedef654fccc4a4d8"}
		},
	]
}
