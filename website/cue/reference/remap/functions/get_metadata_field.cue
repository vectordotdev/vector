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
				The path of the value to look up in the metadata. This must be a VRL path.
				"""
			required: true
			type: ["path"]
		},
	]
	internal_failure_reasons: [
	]
	return: types: ["any"]

	examples: [
		{
			title: "Get a metadata value."
			source: #"""
				get_metadata_field(.my_metadata_field)
				"""#
			return: "abc123"
		},
	]
}
