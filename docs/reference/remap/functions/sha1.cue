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
	internal_failure_reasons: []
	return: ["string"]
	category: "Hash"
	description: #"""
		Calculates a sha1 hash of the provided `value`.
		"""#
	examples: [
		{
			title: "Calculate sha1 hash"
			source: #"""
				sha1("foo")
				"""#
			return: "0beec7b5ea3f0fdbc95d0dd47f3c5bc275da8a33"
		},
	]
}
