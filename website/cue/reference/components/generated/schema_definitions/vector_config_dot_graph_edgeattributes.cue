package metadata

_schemaDefinitions: "vector::config::dot_graph::EdgeAttributes": object: {
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
