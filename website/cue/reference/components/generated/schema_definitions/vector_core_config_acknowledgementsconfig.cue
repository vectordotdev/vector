package metadata

_schemaDefinitions: "vector_core::config::AcknowledgementsConfig": object: options: enabled: {
	description: """
		Controls whether or not end-to-end acknowledgements are enabled.

		When enabled for a sink, any source that supports end-to-end
		acknowledgements that is connected to that sink waits for events
		to be acknowledged by **all connected sinks** before acknowledging them at the source.

		Enabling or disabling acknowledgements at the sink level takes precedence over any global
		[`acknowledgements`][global_acks] configuration.

		[global_acks]: https://vector.dev/docs/reference/configuration/global-options/#acknowledgements
		"""
	required: false
	type: bool: {}
}
