package metadata

remap2: example: add_fields: {
	title: "Add fields"
	source: #"""
		.new_field = "Hello, World!"
		"""#
	output: log: new_field: "Hello, World!"
}
