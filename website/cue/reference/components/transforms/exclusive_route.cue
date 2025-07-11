package metadata

components: transforms: exclusive_route: {
	title: "Exclusive Route"

	description: """
		Routes events from one or more streams to unique sub-streams based on a set of user-defined conditions.

		Also, see the [Route](\(urls.vector_route_transform)) transform for routing an event to multiple streams.
		"""

	classes: {
		commonly_used: false
		development:   "beta"
		egress_method: "stream"
		stateful:      false
	}

	features: {
		exclusive_route: {}
	}

	support: {
		requirements: []
		warnings: []
		notices: []
	}

	configuration: generated.components.transforms.exclusive_route.configuration

	input: {
		logs: true
		metrics: {
			counter:      true
			distribution: true
			gauge:        true
			histogram:    true
			set:          true
			summary:      true
		}
		traces: true
	}

	output: {
		logs: "": {
			description: "The input `log` event."
		}
		metrics: "": {
			description: "The input `metric` event."
		}
		traces: "": {
			description: "The input `trace` event."
		}
	}

	outputs: [
		{
			name:        "<route_id>"
			description: "Each route can be referenced as an input by other components with the name `<transform_name>.<route_id>`."
		},
	]

	how_it_works: {
		routing_to_multiple_components: {
			title: "Routing to multiple components"
			body: """
				An event can only be routed to a single output.
				The following is an example of how you can create two exclusive routes (plus the implicitly created `_unmatched` route).

				```yaml
				transforms:
					transform0:
						inputs:
							- source0
						type: exclusive_route
						routes:
							- name: "a"
								condition:
									type: vrl
									source: .level == 1
							- name: "b"
								condition:
									type: vrl
									# Note that the first condition is redundant. The previous route will always have precedence.
									source: .level == 1 || .level == 2

				tests:
					- name: case-1
						inputs:
							- type: log
								insert_at: transform0
								log_fields:
									level: 1
							- type: log
								insert_at: transform0
								log_fields:
									level: 2
						outputs:
							- extract_from: transform0.a
								conditions:
									- type: vrl
										source: |
											assert!(.level == 1)
							- extract_from: transform0.b
								conditions:
									- type: vrl
										source: |
											assert!(.level == 2)
				```
				"""
		}
	}
}
