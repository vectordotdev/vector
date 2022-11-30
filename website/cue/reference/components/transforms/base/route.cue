package metadata

base: components: transforms: route: configuration: route: {
	description: """
		A table of route identifiers to logical conditions representing the filter of the route.

		Each route can then be referenced as an input by other components with the name
		`<transform_name>.<route_id>`. If an event doesn’t match any route, it will be sent to the
		`<transform_name>._unmatched` output.

		Both `_unmatched`, as well as `_default`, are reserved output names and cannot be used as a
		route name.
		"""
	required: false
	type: object: options: "*": {
		description: """
			An event matching condition.

			Many methods exist for matching events, such as using a VRL expression, a Datadog Search query string,
			or hard-coded matchers like "must be a metric" or "fields A, B, and C must match these constraints".

			As VRL is the most common way to apply conditions to events, this type provides a shortcut to define VRL expressions
			directly in configuration by passing the VRL expression as a string:

			```toml
			condition = '.message == "hooray"'
			```

			When other condition types are required, they can specified with an enum-style notation:

			```toml
			condition.type = 'datadog_search'
			condition.source = 'NOT "foo"'
			```
			"""
		required: true
		type: condition: {}
	}
}
