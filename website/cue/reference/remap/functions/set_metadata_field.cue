package metadata

remap: functions: set_metadata_field: {
	category: "Event"
	description: """
		Sets the given field in the event metadata to the provided value. This can utilize VRL paths and store
		arbitrarily typed metadata on an event.
		"""

	arguments: [
		{
			name:        "key"
			description: "The path of the value to set in the metadata. This must be a VRL path string literal."
			required:    true
			type: ["string"]
		},
		{
			name:        "value"
			description: "The value to set the field to."
			required:    true
			type: ["any"]
		},
	]
	internal_failure_reasons: [
	]
	return: types: ["null"]

	examples: [
		{
			title: "Set the Datadog API key in the event metadata to the given value."
			source: #"""
				set_metadata_field!("datadog_api_key", "abc122")
				"""#
			return: "null"
		},
		{
			title: "Sets arbitrary metadata on an event."
			source: #"""
				value = {"message": "Any VRL type can be used"}
				set_metadata_field("nested.foo.bar", value)
				"""#
			return: "null"
		},
	]
}
