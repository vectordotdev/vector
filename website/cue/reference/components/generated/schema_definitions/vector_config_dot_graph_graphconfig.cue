package metadata

_schemaDefinitions: "vector::config::dot_graph::GraphConfig": object: options: {
	edge_attributes: {
		description: """
			Edge attributes to add to the edges linked to this component's node in resulting graph

			They are added to the edge as provided
			"""
		required: false
		type:     _schemaDefinitions["derived::37368e0c3bfd480d7d049922"]
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
