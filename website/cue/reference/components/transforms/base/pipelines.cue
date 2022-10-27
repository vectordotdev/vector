package metadata

base: components: transforms: pipelines: configuration: {
	logs: {
		description: "Configuration for the logs-specific side of the pipeline."
		required:    false
		type: array: {
			default: []
			items: type: object: options: {
				filter: {
					description: "A logical condition used to determine if an event should be processed by this pipeline."
					required:    false
					type: condition: {}
				}
				name: {
					description: "The name of the pipeline."
					required:    true
					type: string: syntax: "literal"
				}
				transforms: {
					description: "A list of sequential transforms that will process any event that is passed to the pipeline."
					required:    false
					type:        "blank"
				}
			}
		}
	}
	metrics: {
		description: "Configuration for the metrics-specific side of the pipeline."
		required:    false
		type: array: {
			default: []
			items: type: object: options: {
				filter: {
					description: "A logical condition used to determine if an event should be processed by this pipeline."
					required:    false
					type: condition: {}
				}
				name: {
					description: "The name of the pipeline."
					required:    true
					type: string: syntax: "literal"
				}
				transforms: {
					description: "A list of sequential transforms that will process any event that is passed to the pipeline."
					required:    false
					type:        "blank"
				}
			}
		}
	}
	traces: {
		description: "Configuration for the traces-specific side of the pipeline."
		required:    false
		type: array: {
			default: []
			items: type: object: options: {
				filter: {
					description: "A logical condition used to determine if an event should be processed by this pipeline."
					required:    false
					type: condition: {}
				}
				name: {
					description: "The name of the pipeline."
					required:    true
					type: string: syntax: "literal"
				}
				transforms: {
					description: "A list of sequential transforms that will process any event that is passed to the pipeline."
					required:    false
					type:        "blank"
				}
			}
		}
	}
}
