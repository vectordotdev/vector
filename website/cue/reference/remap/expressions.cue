package metadata

remap: {
	#Expression: {
		anchor:      name
		name:        string
		title:       string
		description: string
		return:      string

		grammar?:         #Grammar
		characteristics?: remap.#Characteristics
		examples: [remap.#Example, ...remap.#Example]
	}

	#Grammar: {
		source: string
		definitions: [Name=string]: {
			name:             Name
			description:      string
			characteristics?: remap.#Characteristics
			enum?:            #Enum
			examples?: [string, ...string]
		}
	}

	expressions: [Name=string]: #Expression & {
		name: Name
	}
}
