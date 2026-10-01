package metadata

_schemaDefinitions: "vector::sources::splunk_hec::CodecConfig": object: options: {
	decoding: {
		description: """
			Decoding configuration applied to the payload.

			When unset, the endpoint preserves its existing per-endpoint default
			behavior. When set, the endpoint-selected payload is processed through
			`framing` and `decoding`, and a single payload can fan out to multiple
			events.
			"""
		required: false
		type:     _schemaDefinitions["codecs::decoding::DeserializerConfig"]
	}
	framing: {
		description: """
			Framing configuration applied to the payload.

			Only used when `decoding` is also set. Defaults to a per-codec choice
			(typically `bytes`) that produces one event per payload.
			"""
		required: false
		type:     _schemaDefinitions["codecs::decoding::FramingConfig"]
	}
}
