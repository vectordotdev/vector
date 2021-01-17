package metadata

remap: concepts: {
	#Concept: {
		anchor:      "#\(name)"
		name:        string
		title:       string
		description: string

		characteristics: [Name=string]: remap.#Characteristic
	}

	{[Name=string]: #Concept & {
		name: Name
	}}
}
