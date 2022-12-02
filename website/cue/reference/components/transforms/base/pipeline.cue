package metadata

base: components: transforms: pipeline: configuration: {
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
