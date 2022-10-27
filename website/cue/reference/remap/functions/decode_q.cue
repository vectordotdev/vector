package metadata

remap: functions: decode_mime_q: {
	category:    "Codec"
	description: """
		Replaces q-encoded or base64-encoded [encoded-word](\(urls.encoded_word)) substrings in the `value` with their original string.
		"""

	arguments: [
		{
			name:        "value"
			description: "The string with [encoded-words](\(urls.encoded_word)) to decode."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: [
		"`value` has invalid encoded [encoded-word](\(urls.encoded_word)) string.",
	]
	return: types: ["string"]

	examples: [
		{
			title: "Decode single encoded-word"
			source: """
				decode_mime_q!("=?utf-8?b?SGVsbG8sIFdvcmxkIQ==?=")
				"""
			return: "Hello, World!"
		},
		{
			title: "Embedded"
			source: """
				decode_mime_q!("From: =?utf-8?b?SGVsbG8sIFdvcmxkIQ==?= <=?utf-8?q?hello=5Fworld=40example=2ecom?=>")
				"""
			return: "From: Hello, World! <hello_world@example.com>"
		},
		{
			title: "Without charset"
			source: """
				decode_mime_q!("?b?SGVsbG8sIFdvcmxkIQ==")
				"""
			return: "Hello, World!"
		},
	]
}
