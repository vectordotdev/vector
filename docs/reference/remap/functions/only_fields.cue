package metadata

remap: functions: only_fields: {
	arguments: [
		{
			name:        "paths"
			description: "The paths of the fields to keep."
			required:    true
			multiple:    true
			type: ["string"]
		},
	]
	internal_failure_reasons: []
	return: ["null"]
	category: "Event"
	description: #"""
		Remove any fields that are *not* specified by the given `paths` from the root `event` object. Multiple fields can be specified.
		"""#
	examples: [
		{
			title: "White-list event fields"
			input: log: {
				field1: 1
				field2: 2
				field3: 3
			}
			source: #"""
				only_fields(.field1, .field3)
				"""#
			output: log: {
				field1: 1
				field3: 3
			}
		},
	]
}
