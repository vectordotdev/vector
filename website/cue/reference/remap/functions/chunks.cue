package metadata

remap: functions: chunks: {
	category: "Array"
	description: """
		Chunks `value` into slices of length `chunk_size` bytes
		"""

	arguments: [
		{
			name:        "value"
			description: "The array of bytes to split."
			required:    true
			type: ["array", "string", "bytes"]
		},
		{
			name:        "chunk_size"
			description: "The desired length of each chunk in bytes."
			required:    false
			default:     0
			type: ["integer"]
		},
	]
	internal_failure_reasons: [
		"`chunk_size` must be a valid usize for this target architecture",
		"`chunk_size` must be at least 1 byte",
	]
	return: {
		types: ["array"]
		rules: [
			"`chunks` is considered fallible if the platform architecture's usize integer is smaller than 64 bits",
			"`chunks` is considered fallible if the supplied `chunk_size` is an expression, and infallible if it's a literal integer.",
		]
	}

	examples: [
		{
			title: "Split a string into chunks"
			source: #"""
				chunks("abcdefgh", 4)
				"""#
			return: ["abcd", "efgh"]
		},
		{
			title: "Chunks do not respect unicode code point boundaries"
			source: #"""
				chunks("ab你好", 4)
				"""#
			return: ["ab�","�好"]
		},
	]
}
