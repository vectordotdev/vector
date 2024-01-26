package metadata

base: components: sinks: unit_test: configuration: {
	test_name: {
		description: "Name of the test that this sink is being used for."
		required:    true
		type: string: {}
	}
	transform_ids: {
		description: "List of names of the transform/branch associated with this sink."
		required:    true
		type: array: items: type: string: {}
	}
}
