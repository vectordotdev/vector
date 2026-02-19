package metadata

remap: functions: xxhash: {
	category:    "Checksum"
	description: """
		Calculates a [xxHash](\(urls.xxhash_rust)) hash of the `value`.
		**Note**: Due to limitations in the underlying VRL data types, this function converts the unsigned 64-bit integer hash result to a signed 64-bit integer. Results higher than the signed 64-bit integer maximum value wrap around to negative values. For the XXH3-128 hash algorithm, values are returned as a string.
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
			description: "The xxHash hashing algorithm to use."
			required:    false
			type: ["string"]
			default: "XXH32"
		},
	]
	internal_failure_reasons: []
	return: types: ["integer", "string"]

	examples: [
		{
			title: "Calculate a hash using the default (XXH32) algorithm"
			source: #"""
				xxhash("foo")
				"""#
			return: 3792637401
		},
		{
			title: "Calculate a hash using the XXH32 algorithm"
			source: #"""
				xxhash("foo", "XXH32")
				"""#
			return: 3792637401
		},
		{
			title: "Calculate a hash using the XXH64 algorithm"
			source: #"""
				xxhash("foo", "XXH64")
				"""#
			return: 3728699739546630719
		},
		{
			title: "Calculate a hash using the XXH3-64 algorithm"
			source: #"""
				xxhash("foo", "XXH3-64")
				"""#
			return: -6093828362558603894
		},
		{
			title: "Calculate a hash using the XXH3-128 algorithm"
			source: #"""
				xxhash("foo", "XXH3-128")
				"""#
			return: "161745101148472925293886522910304009610"
		},
	]
}
