package metadata

remap: functions: encode_punycode: {
	category:    "Codec"
	description: """
		Encodes a `value` to [punycode](\(urls.punycode)). Useful for internationalized domain names ([IDN](\(urls.idn))).
		"""

	arguments: [
		{
			name:        "value"
			description: "The string to encode."
			required:    true
			type: ["string"]
		},
		{
			name:        "validate"
			description: "Whether to validate the input string to check if it is a valid domain name."
			required:    false
			type: ["boolean"]
			default: true
		},
	]
	internal_failure_reasons: [
		"`value` can not be encoded to `punycode`",
	]
	return: types: ["string"]

	examples: [
		{
			title: "Encode an internationalized domain name"
			source: """
				encode_punycode!("www.café.com")
				"""
			return: "www.xn--caf-dma.com"
		},
		{
			title: "Encode an internationalized domain name with mixed case"
			source: """
				encode_punycode!("www.CAFé.com")
				"""
			return: "www.xn--caf-dma.com"
		},
		{
			title: "Encode an ASCII only string"
			source: """
				encode_punycode!("www.cafe.com")
				"""
			return: "www.cafe.com"
		},
		{
			title: "Ignore validation"
			source: """
				encode_punycode!("xn--8hbb.xn--fiba.xn--8hbf.xn--eib.", validate: false)
				"""
			return: "xn--8hbb.xn--fiba.xn--8hbf.xn--eib."
		},
	]
}
