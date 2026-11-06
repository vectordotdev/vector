package metadata

_schemaDefinitions: "codecs::encoding::format::json::JsonSerializerOptions": object: options: {
	bytes_format: {
		description: """
			Controls how binary data in string values is encoded.

			String values can hold arbitrary bytes that are not valid UTF-8, such as binary WebSocket
			frames. This option applies to every string value in log and trace events, including
			fields such as `host` and `source_type`, not only to those that hold binary data. Object
			keys, timestamps, and metric events are not affected, and neither are fields that a sink
			writes outside the encoded event, such as the Splunk HEC `fields`.
			"""
		required: false
		type: string: {
			default: "lossy_utf8"
			enum: {
				base64: """
					Encode strings as [standard padded base64][rfc4648], the same output as the VRL
					`encode_base64` function with its default options. All bytes are preserved, so consumers
					must base64-decode every string value. Encoded strings are about a third larger.

					[rfc4648]: https://datatracker.ietf.org/doc/html/rfc4648#section-4
					"""
				lossy_utf8: """
					Encode strings as UTF-8, replacing invalid UTF-8 sequences with the
					[`U+FFFD REPLACEMENT CHARACTER`][U+FFFD]. Bytes that are not valid UTF-8 cannot be
					recovered.

					[U+FFFD]: https://en.wikipedia.org/wiki/Specials_(Unicode_block)#Replacement_character
					"""
			}
		}
	}
	pretty: {
		description: "Whether to use pretty JSON formatting."
		required:    false
		type: bool: default: false
	}
}
