package metadata

base: components: transforms: route: configuration: route: {
	description: """
		A table of route identifiers to logical conditions representing the filter of the route.

		Each route can then be referenced as an input by other components with the name
		`<transform_name>.<route_id>`. If an event doesn’t match any route, it is sent to the
		`<transform_name>._unmatched` output.

		Both `_unmatched`, as well as `_default`, are reserved output names and thus cannot be used
		as a route name.
		"""
	required: false
	type: object: options: "*": {
		description: "An individual route."
		required:    true
		type: condition: {}
	}
}
