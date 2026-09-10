package metadata

_schemaDefinitions: "codecs::encoding::format::gelf::GelfSerializerOptions": object: options: max_chunk_size: {
	description: """
		Maximum size for each GELF chunked datagram (including 12-byte header).
		Chunking starts when datagrams exceed this size.
		For Graylog target, keep at or below 8192 bytes; for Vector target (`gelf` decoding with `chunked_gelf` framing), up to 65,500 bytes is recommended.
		"""
	required: false
	type: uint: default: 8192
}
