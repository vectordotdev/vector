package metadata

_schemaDefinitions: "bool_or_struct::vector_core::config::SourceAcknowledgementsConfig": {
	bool: {}
	object: options: enabled: {
		description: "Whether or not end-to-end acknowledgements are enabled for this source."
		required:    false
		type: bool: {}
	}
}
