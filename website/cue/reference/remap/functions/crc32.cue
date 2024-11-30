package metadata

remap: functions: crc32: {
	category: "Checksum"
	description: """
		Calculates a CRC32 of the `value`.
		"""

	arguments: [
		{
			name:        "value"
			description: "The string to calculate the checksum for."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: []
	return: types: ["string"]

	examples: [
		{
			title: "Create CRC32 checksum"
			source: #"""
				crc32("foo")
				"""#
			return: "8c736521"
		},
	]
}
