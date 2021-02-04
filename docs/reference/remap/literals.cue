package metadata

remap: {
	#Literal: {
		anchor:          name
		name:            string
		title:           string
		description:     string
		characteristics: remap.#Characteristics
		examples: [string, ...string]
	}

	literals: [Name=string]: #Literal & {
		name: Name
	}
}
