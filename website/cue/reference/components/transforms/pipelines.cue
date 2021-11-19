package metadata

components: transforms: pipelines: {
	title: "Pipelines"

	description: """
		Defines an ordered chain of child pipelines, split by event type (logs and metrics),
		in which a chain of child transforms is defined, that will be applied sequentially
		on incoming events.
		"""

	classes: {
		commonly_used: false
		development:   "beta"
		egress_method: "stream"
		stateful:      false
	}

	features: {}

	support: {
		targets: {
			"aarch64-unknown-linux-gnu":      true
			"aarch64-unknown-linux-musl":     true
			"armv7-unknown-linux-gnueabihf":  true
			"armv7-unknown-linux-musleabihf": true
			"x86_64-apple-darwin":            true
			"x86_64-pc-windows-msv":          true
			"x86_64-unknown-linux-gnu":       true
			"x86_64-unknown-linux-musl":      true
		}
		requirements: []
		warnings: []
		notices: []
	}

	configuration: {
		_pipeline_group: {
			description: """
				A list of pipeline's configurations. It's also possible to define this order in which the pipelines are
				chained using the `order` options. If no order is specified, they will be chained by alphabetical order.
				"""
			required: true
			warnings: []
			type: object: options: {
				order: {
					common:      true
					description: "A complete ordered list of how the pipelines will be chained."
					required:    false
					type: array: {
						default: null
						items: type: string: {
							syntax: "literal"
						}
					}
				}
				pipelines: {
					description: "A set of pipelines describing how the events will be transformed."
					required:    true
					type: object: options: {
						"*": _pipeline_configuration
					}
				}
			}
		}

		_pipeline_configuration: {
			description: """
				Any valid transform configuration. See [transforms documentation](\(urls.vector_transforms))
				for the list of available transforms and their configuration.
				"""
			required:    true
			warnings: []
			type: object: options: {
				name: {
					description: "Name of the pipeline"
					required:    false
					common:      true
					type: string: default: null
				}

				filter: {
					description: """
						A condition to filter the events that will be processed by the pipeline. If the conditions is not satisfied,
						the event will be forwarded to the next pipeline.

						The filter uses the same format that conditions use for [unit testing](\(urls.vector_unit_tests)).
						"""
					required:    false
					common:      true
					type: string: {
						default: "vrl"

						enum: {
							vrl: "[Vector Remap Language](\(urls.vrl_reference))."
						}
					}
				}

				transforms: {
					description: """
						Any list of valid transform configurations. See [transforms documentation](\(urls.vector_transforms))
						for the list of available transforms and their configuration.
						"""
					required:    true
					type: array: items: type: object: options: {}
				}
			}
		}

		logs:    _pipeline_group
		metrics: _pipeline_group
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
			title: "Filter by log level and reformat"
			configuration: """
				[transforms.pipelines_processing]
				type = "pipelines"
				inputs = ["pipelines_gate"]

				[transforms.pipelines_processing.logs]
				order = [
				  "foo",
				  "bar"
				]

				[transforms.pipelines_processing.logs.pipelines.foo]
				name = "foo"
				filter.type = "vrl"
				filter.source = '''
					contains(.message, "hello") ?? false
				'''

				[[transforms.pipelines_processing.logs.pipelines.foo.transforms]]
				type = "remap"
				source = '''
				.message = "[foo]" + .message
				'''

				[[transforms.pipelines_processing.logs.pipelines.foo.transforms]]
				type = "remap"
				source = ".went_through_foo = true"

				[transforms.pipelines_processing.logs.pipelines.bar]
				name = "bar"

				[[transforms.pipelines_processing.logs.pipelines.foo.transforms]]
				type = "remap"
				source = '''
				.message = "[bar]" + .message
				'''

				[[transforms.pipelines_processing.logs.pipelines.bar.transforms]]
				type = "remap"
				source = ".went_through_bar = true"
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
			]
			output: [
				{
					log: {
						message:          "[foo][bar] hello"
						went_through_foo: true
						went_through_bar: true
					}
				},
				{
					log: {
						message:          "[foo][bar] world"
						went_through_bar: true
					}
				},
			]
		},
	]
}
