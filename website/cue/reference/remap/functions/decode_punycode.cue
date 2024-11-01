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
		{
			name:        "validate"
			description: "If enabled, checks if the input string is a valid domain name."
			required:    false
			type: ["boolean"]
			default: true
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
		{
			title: "Ignore validation"
			source: """
				decode_punycode!("xn--8hbb.xn--fiba.xn--8hbf.xn--eib.", validate: false)
				"""
			return: "١٠.٦٦.٣٠.٥."
		},
	]
}
