package metadata

remap: functions: get_metadata_field: {
	category: "Event"
	description: """
		Returns the value of the given field from the event metadata. This can utilize VRL paths and store
		arbitrarily typed metadata on an event.
		"""

	arguments: [
		{
			name: "key"
			description: """
				The path of the value to look up in the metadata. This must be a VRL path string literal.
				"""
			required: true
			type: ["string"]
		},
	]
	internal_failure_reasons: [
	]
	return: types: ["any"]

	examples: [
		{
			title: "Get the Datadog API key from the event metadata."
			source: #"""
				get_metadata_field("datadog_api_key")
				"""#
			return: "abc123"
		},
	]
}
