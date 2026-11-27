package metadata

_schemaDefinitions: "codecs::decoding::format::gelf::GelfDeserializerOptions": object: options: {
	decompression: {
		description: """
			Configures the decompression of compressed messages.

			Compressed messages are decompressed before parsing. Set this to `None` to reject
			them. For datagram inputs, also set `framing.chunked_gelf.decompression` to `None`
			to disable decompression in the framing.
			"""
		required: false
		type: string: {
			default: "Auto"
			enum: {
				Auto: "Detect compression from the magic bytes and decompress before parsing."
				None: "Do not decompress; compressed messages are rejected as invalid JSON."
			}
		}
	}
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
