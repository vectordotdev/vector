package metadata

components: transforms: switch: {
	title: "Switch"

	description: """
		Splits a stream of events into multiple sub-streams based on a set of
		conditions.
		Contrary to the route transform, the event will only be sent to the first
		matching case and therefore the event will not be cloned.
		"""

	classes: {
		commonly_used: false
		development:   "stable"
		egress_method: "stream"
		stateful:      false
	}

	support: {
		requirements: []
		warnings: []
		notices: []
	}

	configuration: {
		cases: {
			description: """
				A list of case identifiers to logical conditions representing the filter of the route. Each case
				can then be referenced as an input by other components with the name `<transform_name>.case_<index>`.
				If no case is matching, then the output name `default` will be used.
				"""
			required: true
			type: array: items: type: object: {
				options: {
					"*": {
						description: """
							The condition to be matched against every input event that didn't match the previous case.
							Only messages that pass the condition will be sent through this route.
							"""
						required: true
						type: condition: {}
					}
				}
			}
		}
	}

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
	}

	examples: [
		{
			title: "Split by log content"

			configuration: """
				[transforms.my_switch]
				type = "switch"
				inputs = ["some_source"]
				
				[[transforms.my_switch.case]]
				type = "vrl"
				source = '''contains(.message, "hello") ?? false'''
				
				[[transforms.my_switch.case]]
				type = "vrl"
				source = '''contains(.message, "world") ?? false'''
				"""

			input: [
				{
					log: {
						message: "hello"
					}
				},
				{
					log: {
						message: "world"
					}
				},
				{
					log: {
						message: "noop"
					}
				},
			]
			output: [
				{
					log: {
						message: "hello"
					}
				},
				{
					log: {
						message: "world"
					}
				},
				{
					log: {
						message: "noop"
					}
				},
			]
		},
	]

	outputs: [
		{
			name:        "case_<index>"
			description: "Each case can be referenced as an input by other components with the name `<transform_name>.case_<index>`."
		},
		{
			name:        "default"
			description: "When none of the cases match the event, can be referenced with the name `<transform_name>.default`."
		},
	]
}
