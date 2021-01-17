package metadata

remap2: concepts: {
	#Concept: {
		anchor:      "#\(name)"
		name:        string
		title:       string
		description: string

		characteristics: [Name=string]: remap2.#Characteristic
	}

	{[Name=string]: #Concept & {
		name: Name
	}}
}
