package metadata

components: transforms: [Name=string]: {
	kind: "transform"

	// Example uses for the component.
	examples: {
		log: [
			...{
				input: #Fields | [#Fields, ...]
			},
		]
	}
}
