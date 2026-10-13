package metadata

_schemaDefinitions: "vector::internal_events::file::FileInternalMetricsConfig": object: options: include_file_tag: {
	description: """
		Whether or not to include the "file" tag on the component's corresponding internal metrics.

		This is useful for distinguishing between different files while monitoring. However, the tag's
		cardinality is unbounded.
		"""
	required: false
	type: bool: default: false
}
