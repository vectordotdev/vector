package metadata

remap: functions: encode_base64: {
	arguments: [
		{
			name:        "value"
			description: "The string to encode."
			required:    true
			type: ["string"]
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
	]
}
