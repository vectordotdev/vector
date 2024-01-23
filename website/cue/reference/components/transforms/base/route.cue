package metadata

base: components: transforms: route: configuration: {
	reroute_unmatched: {
		description: """
			Reroutes unmatched events to a named output instead of silently discarding them.

			Normally, if an event doesn't match any defined route, it is sent to the `<transform_name>._unmatched`
			output for further processing. In some cases, you may want to simply discard unmatched events and not
			process them any further.

			In these cases, `reroute_unmatched` can be set to `false` to disable the `<transform_name>._unmatched`
			output and instead silently discard any unmatched events.
			"""
		required: false
		type: bool: default: true
	}
	route: {
		description: """
			A table of route identifiers to logical conditions representing the filter of the route.

			Each route can then be referenced as an input by other components with the name
			`<transform_name>.<route_id>`. If an event doesn’t match any route, and if `reroute_unmatched`
			is set to `true` (the default), it is sent to the `<transform_name>._unmatched` output.
			Otherwise, the unmatched event is instead silently discarded.

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
}
