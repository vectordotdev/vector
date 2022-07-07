package metadata

remap: functions: remove_metadata_field: {
	category: "Event"
	description: """
		Removes the value of the given field from the event metadata. This can utilize VRL paths.
		"""

	arguments: [
		{
			name: "key"
			description: """
				The path to the metadata value to remove. This must be a VRL path.
				"""
			required: true
			type: ["path"]
		},
	]
	internal_failure_reasons: [
	]
	return: types: ["null"]

	examples: [
		{
			title: "Removes metadata."
			source: #"""
				remove_metadata_field(.my_metadata_field)
				"""#
			return: "null"
		},
	]
}
