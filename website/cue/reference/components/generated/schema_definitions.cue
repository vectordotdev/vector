package metadata

#SchemaDefinitions: chunked_gelf_decoder_options: object: options: {
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
			dropped. If this option is not set, the decoder does not limit the length of messages and
			the per-message memory is unbounded.

			**Note**: A message can be composed of multiple chunks, and this limit applies to the whole
			message, not to individual chunks.

			This limit takes into account only the message payload. GELF header bytes are excluded from the calculation.
			The message payload is the concatenation of all chunk payloads.
			"""
		required: false
		type: uint: {}
	}
	pending_messages_limit: {
		description: """
			The maximum number of pending incomplete messages. If this limit is reached, the decoder starts
			dropping chunks of new messages, ensuring the memory usage of the decoder's state is bounded.
			If this option is not set, the decoder does not limit the number of pending messages and the memory usage
			of its messages buffer can grow unbounded. This matches Graylog Server's behavior.
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
