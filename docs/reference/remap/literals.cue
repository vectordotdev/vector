package metadata

remap: literals: {
	#Literal: {
		anchor:      "#\(name)"
		name:        string
		title:       string
		description: string
		characteristics: [Name=string]: remap.#Characteristic
		examples: [string, ...string]
	}

	{[Name=string]: #Literal & {
		name: Name
	}}
}
