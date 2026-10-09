package metadata

_schemaDefinitions: "codecs::decoding::framing::character_delimited::CharacterDelimitedDecoderOptions": object: options: {
	delimiter: {
		description: "The character that delimits byte sequences."
		required:    true
		type: ascii_char: {}
	}
	max_length: {
		description: """
			The maximum length of the byte buffer.

			This length does *not* include the trailing delimiter.

			By default, no maximum length is enforced. If events are malformed, this can lead to
			additional resource usage as events continue to be buffered in memory, and can potentially
			lead to memory exhaustion in extreme cases.

			If there is a risk of processing malformed data, such as logs with user-controlled input,
			consider setting the maximum length to a reasonably large value as a safety net. This
			prevents processing from being unbounded.
			"""
		required: false
		type: uint: {}
	}
	oversized_action: {
		description: """
			The behavior when a frame exceeds `max_length`.

			When set to `drop` (the default), the entire oversized frame is discarded.
			When set to `truncate`, the frame is truncated to `max_length` bytes and the
			remainder is discarded up to the next delimiter.

			This option has no effect if `max_length` is not set.
			"""
		required: false
		type: string: {
			default: "drop"
			enum: {
				drop: "Drop the entire oversized frame."
				truncate: """
					Truncate the frame to the maximum allowed size and emit the partial content.

					The remainder of the oversized frame is discarded up to the next delimiter.
					"""
			}
		}
	}
}
