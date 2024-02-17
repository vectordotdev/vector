package metadata

remap: functions: decode_punycode: {
	category:    "Codec"
	description: """
		Decodes a [punycode](\(urls.punycode)) encoded `value`, like an internationalized domain name ([IDN](\(urls.idn))).
		"""

	arguments: [
		{
			name:        "value"
			description: "The string to decode."
			required:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: [
		"`value` is not valid `punycode`",
	]
	return: types: ["string"]

	examples: [
		{
			title: "Decode a punycode encoded internationalized domain name"
			source: """
				decode_punycode!("www.xn--caf-dma.com")
				"""
			return: "www.café.com"
		},
		{
			title: "Decode an ASCII only string"
			source: """
				decode_punycode!("www.cafe.com")
				"""
			return: "www.cafe.com"
		},
	]
}
