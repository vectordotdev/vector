package metadata

remap2: examples: rename_fields: {
	title: "Rename fields"
	input: log: old_field: "value"
	source: #"""
		.new_field = del(.old_field)
		"""#
	output: log: new_field: "value"
}
