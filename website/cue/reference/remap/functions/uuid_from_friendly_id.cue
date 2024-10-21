package metadata

remap: functions: uuid_from_friendly_id: {
	category: "Random"
	description: """
		Convert a Friendly ID (base62 encoding a 128-bit word) to a UUID.
		"""

	arguments: [
		{
			name:        "value"
			description: "A string that is a Friendly ID"
			required:    true
			type: ["timestamp"]
		},
	]
	internal_failure_reasons: [
		"`value` is a string but the text uses characters outside of class [0-9A-Za-z].",
		"`value` is a base62 encoding of an integer, but the integer is greater than or equal to 2^128.",
	]
	return: types: ["string"]

	examples: [
		{
			title: "Convert a Friendly ID to a UUID"
			source: #"""
				uuid_from_friendly_id!("3s87yEvnmkiPBMHsj8bwwc")
				"""#
			return: "7f41deed-d5e2-8b5e-7a13-ab4ff93cfad2"
		},
	]
}
