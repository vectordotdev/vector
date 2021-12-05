package metadata

remap: {
	#Syntax: {
		name:             string
		title:            string
		description:      string
		characteristics?: remap.#Characteristics
		examples?: [string, ...string]
	}

	syntax: [Name=string]: #Syntax & {
		name: Name
	}
}
