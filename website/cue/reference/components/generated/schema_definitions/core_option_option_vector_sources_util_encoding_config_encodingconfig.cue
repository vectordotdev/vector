package metadata

_schemaDefinitions: "core::option::Option<vector::sources::util::encoding_config::EncodingConfig>": object: options: charset: {
	description: """
		Encoding of the source messages.

		Takes one of the encoding [label strings](https://encoding.spec.whatwg.org/#concept-encoding-get) defined as
		part of the [Encoding Standard](https://encoding.spec.whatwg.org/).

		When set, the messages are transcoded from the specified encoding to UTF-8, which is the encoding that is
		assumed internally for string-like data. Enable this transcoding operation if you need your data to
		be in UTF-8 for further processing. At the time of transcoding, any malformed sequences (that can't be mapped to
		UTF-8) is replaced with the Unicode [REPLACEMENT
		CHARACTER](https://en.wikipedia.org/wiki/Specials_(Unicode_block)#Replacement_character) and warnings are
		logged.
		"""
	required: true
	type: string: examples: ["utf-16le", "utf-16be"]
}
