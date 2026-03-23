package metadata

generated: components: transforms: configuration: {
	graph: {
		description: """
			Extra graph configuration

			Configure output for component when generated with graph command
			"""
		required: false
		type: object: options: {
			edge_attributes: {
				description: """
					Edge attributes to add to the edges linked to this component's node in resulting graph

					They are added to the edge as provided
					"""
				required: false
				type: object: {
					examples: [{
						example_input: {
							color: "red"
							label: "Example Edge"
							width: "5.0"
						}
					}]
					options: "*": {
						description: "A collection of graph edge attributes in graphviz DOT language, related to a single input component."
						required:    true
						type: object: {
							examples: [{
								color: "red"
								label: "Example Edge"
								width: "5.0"
							}]
							options: "*": {
								description: "A single graph edge attribute in graphviz DOT language."
								required:    true
								type: string: {}
							}
						}
					}
				}
			}
			node_attributes: {
				description: """
					Node attributes to add to this component's node in resulting graph

					They are added to the node as provided
					"""
				required: false
				type: object: {
					examples: [{
						color: "red"
						name:  "Example Node"
						width: "5.0"
					}]
					options: "*": {
						description: "A single graph node attribute in graphviz DOT language."
						required:    true
						type: string: {}
					}
				}
			}
		}
	}
	inputs: {
		description: """
			A list of upstream [source][sources] or [transform][transforms] IDs.

			Wildcards (`*`) are supported.

			See [configuration][configuration] for more info.

			[sources]: https://vector.dev/docs/reference/configuration/sources/
			[transforms]: https://vector.dev/docs/reference/configuration/transforms/
			[configuration]: https://vector.dev/docs/reference/configuration/
			"""
		required: true
		type: array: items: type: string: examples: ["my-source-or-transform-id", "prefix-*"]
	}
}
