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
	]
	return: ["string"]
	category:    "Encode"
	description: "Encodes the provided string to [Base64](\(urls.base64))."
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
			source: ".encoded = encode_base64(.message, padding = false)"
			output: {
				message: "please encode me, no padding though"
				encoded: "cGxlYXNlIGVuY29kZSBtZSwgbm8gcGFkZGluZyB0aG91Z2g"
			}
		},
	]
}
