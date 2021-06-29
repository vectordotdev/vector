package metadata

remap: functions: decode_percent: {
	category:    "Codec"
	description: """
		Decodes a [percent-encoded](\(urls.percent_encoded_bytes)) `value` like a URL.
		"""

	arguments: [
		{
			name:        "value"
			description: "The string to decode."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: []
	return: types: ["string"]

	examples: [
		{
			title: "Percent decode a value"
			source: """
				decode_percent("foo%20bar%3F")
				"""
			return: "foo bar?"
		},
	]
}
