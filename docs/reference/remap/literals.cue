package metadata

remap: {
	#Literal: {
		anchor:      "#\(name)"
		name:        string
		title:       string
		description: string
		characteristics: [Name=string]: remap.#Characteristic
		examples: [string, ...string]
	}

	literals: [Name=string]: #Literal & {
		name: Name
	}
}
