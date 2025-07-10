package metadata

components: transforms: route: {
	title: "Route"

	description: """
		Splits a stream of events into multiple sub-streams based on a set of
		conditions.

		Also, see the [Exclusive Route](\(urls.vector_exclusive_route_transform)) transform for routing an event to
		a single stream.
		"""

	classes: {
		commonly_used: false
		development:   "stable"
		egress_method: "stream"
		stateful:      false
	}

	features: {
		route: {}
	}

	support: {
		requirements: []
		warnings: []
		notices: []
	}

	configuration: generated.components.transforms.route.configuration

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

	examples: [
		{
			title: "Split by log level"

			configuration: {
				route: {
					debug: #".level == "debug""#
					info:  #".level == "info""#
					warn:  #".level == "warn""#
					error: #".level == "error""#
				}
			}

			input: log: {
				level: "info"
			}
			output: log: {
				level: "info"
			}
		},
		{
			title: "Split by metric namespace"

			configuration: {
				route: {
					app:  #".namespace == "app""#
					host: #".namespace == "host""#
				}
			}

			input: metric: {
				counter: {
					value: 10000.0
				}
				kind:      "absolute"
				name:      "memory_available_bytes"
				namespace: "host"
			}
			output: metric: {
				counter: {
					value: 10000.0
				}
				kind:      "absolute"
				name:      "memory_available_bytes"
				namespace: "host"
			}
		},
	]

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
				The following is an example of how you can create two routes that feed three downstream components.

				It is worth noting that a single route can feed multiple downstream components.

				```yaml
				transforms:
					my-routes:
						inputs: [ some_source ]
						type: route
						route:
							foo-exists: 'exists(.foo)'
							foo-doesnt-exist: '!exists(.foo)'
					remap-route-1:
						type: remap
						inputs:
							- my-routes.foo-exists
						source: |
							.route = "route 1"
					remap-route-2:
						type: remap
						inputs:
							- my-routes.foo-doesnt-exist
						source: |
							.route = "route 2"
					remap-route-3:
						type: remap
						inputs:
							- my-routes.foo-exists
						source: |
							.route = "route 3"

				tests:
					- name: case-1
						inputs:
							- type: log
								insert_at: my-routes
								log_fields:
									foo: X
						outputs:
							- extract_from: remap-route-1
								conditions:
									- type: vrl
										source: |
											assert!(exists(.foo))
											assert_eq!(.route, "route 1")
							- extract_from: remap-route-3
								conditions:
									- type: vrl
										source: |
											assert!(exists(.foo))
											assert_eq!(.route, "route 3")
					- name: case-2
						inputs:
							- type: log
								insert_at: my-routes
								log_fields:
									bar: X
						outputs:
							- extract_from: remap-route-2
								conditions:
									- type: vrl
										source: |
											assert!(!exists(.foo))
											assert_eq!(.route, "route 2")
				```
				"""
		}
	}
}
