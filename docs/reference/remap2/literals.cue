package metadata

remap2: {
	#Literal: {
		anchor:      "#\(name)"
		name:        string
		title:       string
		description: string
		characteristics: [Name=string]: remap2.#Characteristic
		examples: [string, ...string]
	}

	literals: [Name=string]: #Literal & {
		name: Name
	}
}
