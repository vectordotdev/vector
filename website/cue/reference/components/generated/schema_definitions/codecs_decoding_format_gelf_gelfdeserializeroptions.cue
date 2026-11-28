package metadata

_schemaDefinitions: "codecs::decoding::format::gelf::GelfDeserializerOptions": object: options: {
	lossy: {
		description: """
			Determines whether to replace invalid UTF-8 sequences instead of failing.

			When true, invalid UTF-8 sequences are replaced with the [`U+FFFD REPLACEMENT CHARACTER`][U+FFFD].

			[U+FFFD]: https://en.wikipedia.org/wiki/Specials_(Unicode_block)#Replacement_character
			"""
		required: false
		type: bool: default: true
	}
	validation: {
		description: "Configures the decoding validation mode."
		required:    false
		type: string: {
			default: "strict"
			enum: {
				relaxed: """
					Uses more relaxed validation that skips strict GELF specification checks.

					This mode does not treat specification violations as errors, allowing the decoder
					to accept messages from sources that don't strictly follow the GELF spec.
					"""
				strict: "Uses strict validation that closely follows the GELF spec."
			}
		}
	}
}
