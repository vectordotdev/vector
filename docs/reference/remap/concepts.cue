package metadata

remap: {
	#Concept: {
		anchor:      name
		name:        string
		title:       string
		description: string

		characteristics: remap.#Characteristics
	}

	concepts: [Name=string]: #Concept & {
		name: Name
	}
}
