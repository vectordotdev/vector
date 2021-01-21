package metadata

remap: {
	#Concept: {
		anchor:      "#\(name)"
		name:        string
		title:       string
		description: string

		characteristics: [Name=string]: remap.#Characteristic
	}

	concepts: [Name=string]: #Concept & {
		name: Name
	}
}
