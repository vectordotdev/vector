package metadata

remap: functions: decode_charset: {
	category:    "Codec"
	description: """
	  Decodes the `value` (a non-UTF8 string) to a UTF8 string using the specified [character set](\(urls.charset_standard)).
	  """

	arguments: [
		{
			name:        "value"
			description: "The non-UTF8 string to decode."
			required:    true
			type: ["string"]
		},
		{
			name:        "from_charset"
			description: "The [character set](\(urls.charset_standard)) to use when decoding the data."
			required:    true
			type: ["string"]

		},
	]
	internal_failure_reasons: [
		"`from_charset` isn't a valid [character set](\(urls.charset_standard)).",
	]
	return: types: ["string"]

	examples: [
		{
			title: "Decode EUC-KR string"
			source: """
				decode_charset!(decode_base64!("vsiz58fPvLy/5A=="), "euc-kr")
				"""
			return: "안녕하세요"
		},
		{
			title: "Decode EUC-JP string"
			source: """
				decode_charset!(decode_base64!("pLOk86TLpMGkzw=="), "euc-jp")
				"""
			return: "こんにちは"
		},
		{
			title: "Decode GB2312 string"
			source: """
				decode_charset!(decode_base64!("xOO6ww=="), "gb2312")
				"""
			return: "你好"
		},
	]
}
