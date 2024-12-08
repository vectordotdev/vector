package metadata

remap: functions: encode_charset: {
	category:    "Codec"
	description: """
	  Encodes the `value` (a UTF8 string) to a non-UTF8 string using the specified [character set](\(urls.charset_standard)).
	  """

	arguments: [
		{
			name:        "value"
			description: "The UTF8 string to encode."
			required:    true
			type: ["string"]
		},
		{
			name:        "to_charset"
			description: "The [character set](\(urls.charset_standard)) to use when encoding the data."
			required:    true
			type: ["string"]

		},
	]
	internal_failure_reasons: [
		"`to_charset` isn't a valid [character set](\(urls.charset_standard)).",
	]
	return: types: ["string"]

	examples: [
		{
			title: "Encode UTF8 string to EUC-KR"
			source: """
				encode_base64(encode_charset!("안녕하세요", "euc-kr"))
				"""
			return: "vsiz58fPvLy/5A=="
		},
		{
			title: "Encode UTF8 string to EUC-JP"
			source: """
				encode_base64(encode_charset!("こんにちは", "euc-jp"))
				"""
			return: "pLOk86TLpMGkzw=="
		},
		{
			title: "Encode UTF8 string to GB2312"
			source: """
				encode_base64(encode_charset!("你好", "gb2312"))
				"""
			return: "xOO6ww=="
		},
	]
}
