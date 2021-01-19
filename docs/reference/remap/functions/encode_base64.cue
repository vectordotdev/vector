package metadata

remap: functions: encode_base64: {
	arguments: [
		{
			name:        "value"
			description: "The string to encode."
			required:    true
			type: ["string"]
		},
		{
			name:        "padding"
			description: "Whether the Base64 output is [padded](\(urls.base64_padding))."
			required:    false
			type: ["boolean"]
			default: true
		},
		{
			name:        "charset"
			description: ""
			required:    false
			type: ["string"]
			default: "standard"
			enum: {
				standard: "[Standard](\(urls.base64_standard)) Base64 format."
				url_safe: "Modified Base64 for [URL variants](\(urls.base64_url_safe)."
			}
		},
	]
	internal_failure_reasons: []
	return: ["string"]
	category: "Codec"
	description: #"""
		Encodes the provided `value` to [Base64](\(urls.base64)) either padded or non-padded and
		using the specified character set.
		"""#
	examples: [
		{
			title: "Encode to Base64 (default)"
			source: """
				.encoded = encode_base64("please encode me")
				"""
			return: "cGxlYXNlIGVuY29kZSBtZQ=="
		},
		{
			title: "Encode to Base64 (without padding)"
			source: """
				encode_base64("please encode me, no padding though", padding: false)
				"""
			return: "cGxlYXNlIGVuY29kZSBtZSwgbm8gcGFkZGluZyB0aG91Z2g"
		},
		{
			title: "Encode to Base64 (URL safe)"
			source: """
				encode_base64("please encode me, but safe for URLs", charset: "url_safe")
				"""
			return: "cGxlYXNlIGVuY29kZSBtZSwgYnV0IHNhZmUgZm9yIFVSTHM="
		},
	]
}
