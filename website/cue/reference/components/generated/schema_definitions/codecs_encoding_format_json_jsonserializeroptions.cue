package metadata

_schemaDefinitions: "codecs::encoding::format::json::JsonSerializerOptions": object: options: {
	bytes: {
		description: """
			How byte values are written.

			Events store string values as bytes, so this applies to every string value, not only to
			values that hold binary data. Object keys, timestamps, and metric events are not affected.
			"""
		required: false
		type: string: {
			default: "lossy_utf8"
			enum: {
				base64: """
					Writes byte values as base64 strings, using the standard alphabet with padding
					(RFC 4648). Binary data is preserved.
					"""
				lossy_utf8: """
					Writes byte values as UTF-8 strings, replacing each invalid UTF-8 sequence with the
					Unicode replacement character (U+FFFD). Binary data is not preserved.
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
