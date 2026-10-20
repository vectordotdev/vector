package metadata

_schemaDefinitions: "codecs::decoding::framing::chunked_gelf::ChunkedGelfDecoderOptions": object: options: {
	decompression: {
		description: "Decompression configuration for GELF messages."
		required:    false
		type: string: {
			default: "Auto"
			enum: {
				Auto: "Automatically detect the decompression method based on the magic bytes of the message."
				Gzip: "Use Gzip decompression."
				None: "Do not decompress the message."
				Zlib: "Use Zlib decompression."
			}
		}
	}
	max_length: {
		description: """
			The maximum length of a single GELF message, in bytes. Messages longer than this length are
			dropped.

			**Note**: A message can be composed of multiple chunks, and this limit applies to the whole
			message, not to individual chunks.

			This limit takes into account only the message payload. GELF header bytes are excluded from the calculation.
			The message payload is the concatenation of all chunk payloads.

			The decoder also limits the payload buffered across *all* incomplete messages to 128 MiB by
			default. Setting this above 128 MiB raises that aggregate limit to the same value.

			An unchunked message is never buffered, so neither limit applies to it; its size is
			bounded by whatever the source accepts as one frame.
			"""
		required: false
		type: uint: default: 134217728
	}
	pending_messages_limit: {
		description: """
			The maximum number of pending incomplete messages. If this limit is reached, the decoder starts
			dropping chunks of new messages, ensuring the memory usage of the decoder's state is bounded.

			Chunks belonging to messages that are already pending are still accepted once the limit is
			reached, so in-flight messages can complete.

			If unset or `null`, this defaults to 4096.
			"""
		required: false
		type: uint: {}
	}
	timeout_secs: {
		description: """
			The timeout, in seconds, for a message to be fully received. If the timeout is reached, the
			decoder drops all received chunks for the timed-out message.
			"""
		required: false
		type: float: default: 5.0
	}
}
