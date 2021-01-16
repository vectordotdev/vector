package metadata

remap2: examples: remove_fields: {
	title: "Remove fields"
	input: log: old_field: "value"
	source: #"""
		del(.old_field)
		"""#
	output: log: {}
}
