package metadata

remap: functions: decode_base64: {
	arguments: [
		{
			name:        "value"
			description: "The [Base64](\(urls.base64)) data to decode."
			required:    true
			type: ["string"]
		},
	]
	return: ["string"]
	category:    "Decode"
	description: "Decodes the provided [Base64](\(urls.base64)) data to a string."
	examples: [
		{
			title: "Decode Base64 data"
			input: {
				message: "eW91IGhhdmUgc3VjY2Vzc2Z1bGx5IGRlY29kZWQgbWU="
			}
			source: ".decoded = decode_base64(.message)"
			output: {
				message: "eW91IGhhdmUgc3VjY2Vzc2Z1bGx5IGRlY29kZWQgbWU="
				decoded: "you have successfully decoded me"
			}
		},
	]
}
