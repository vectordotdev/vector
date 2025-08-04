package metadata

generated: components: transforms: exclusive_route: configuration: routes: {
	description: "An array of named routes. The route names are expected to be unique."
	required:    true
	type: array: items: type: object: {
		examples: [{
			condition: {
				source: "exists(.foo) && exists(.bar)"
				type:   "vrl"
			}
			name: "foo-and-bar-exist"
		}, {
			condition: {
				source: "exists(.foo)"
				type:   "vrl"
			}
			name: "only-foo-exists"
		}]
		options: {
			condition: {
				description: "Each condition represents a filter which is applied to each event."
				required:    true
				type: condition: {}
			}
			name: {
				description: """
					The name of the route is also the name of the transform port.

					The `_unmatched` name is reserved and thus cannot be used as route ID.

					Each route can then be referenced as an input by other components with the name
					 `<transform_name>.<name>`. If an event doesn’t match any route,
					it is sent to the `<transform_name>._unmatched` output.
					"""
				required: true
				type: string: {}
			}
		}
	}
}
