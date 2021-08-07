package metadata

components: transforms: compound: {
	title: "Compound"

	description: """
		Defines an ordered chain of child tranforms that will be applied sequentially
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
		nested: {
			description: """
				A table of transforms configurations' representing the chain of transforms to be applied on incoming
				events. All transforms in the chain can then be referenced as an input by other components with the name
				`<transform_name>.<nested_transform_name>`. All transforms in the chain also generate internal metrics
				as if they were configured separately.
				"""
			required: true
			warnings: []
			type: object: {
				options: {
					"*": {
						description: """
							Any valid transform configuration. See [transforms documentation](\(urls.vector_transforms))
							for the list of available transforms and their configuration.
							"""
						required:    true
						warnings: []
						type: object: {}
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
			title: "Filter by log level and reformat"
			configuration: """
				[transforms.chain]
				type = "compound"

				[transforms.pipelines.nested.step_1]
				type = "filter"
				condition = '.level == "debug"'

				[transforms.pipelines.nested.step_2]
				type = "remap"
				source = '''
					.message, _ = "[" + del(.level) + "] " +  .message
				'''
				"""
			input: [
				{
					log: {
						level:   "debug"
						message: "I'm a noisy debug log"
					}
				},
				{
					log: {
						level:   "info"
						message: "I'm a normal info log"
					}
				},
			]
			output: [
				{
					log: {
						message: "[debug] I'm a noisy debug log"
					}
				},
			]
		},
	]
}
