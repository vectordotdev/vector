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
	internal_failure_reason: null
	return: ["string"]
	category: "Encode"
	description: #"""
		Encodes the provided `value` to [Base64](\(urls.base64)) either padded or non-padded and
		using the specified character set.
		"""#
	examples: [
		{
			title: "Encode string"
			input: {
				message: "please encode me"
			}
			source: ".encoded = encode_base64(.message)"
			output: {
				message: "please encode me"
				encoded: "cGxlYXNlIGVuY29kZSBtZQ=="
			}
		},
		{
			title: "Encode string without padding"
			input: {
				message: "please encode me, no padding though"
			}
			source: ".encoded = encode_base64(.message, padding: false)"
			output: {
				message: "please encode me, no padding though"
				encoded: "cGxlYXNlIGVuY29kZSBtZSwgbm8gcGFkZGluZyB0aG91Z2g"
			}
		},
		{
			title: "Encode URL string"
			input: {
				message: "please encode me, but safe for URLs"
			}
			source: #".encoded = encode_base64(.message, charset: "url_safe")"#
			output: {
				message: "please encode me, but safe for URLs"
				encoded: "cGxlYXNlIGVuY29kZSBtZSwgYnV0IHNhZmUgZm9yIFVSTHM="
			}
		},
	]
}
