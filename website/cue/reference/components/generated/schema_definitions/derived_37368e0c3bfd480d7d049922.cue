package metadata

_schemaDefinitions: "derived::37368e0c3bfd480d7d049922": object: {
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
		type:        _schemaDefinitions["vector::config::dot_graph::EdgeAttributes"]
	}
}
