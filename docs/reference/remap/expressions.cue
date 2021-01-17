package metadata

remap: {
	#Expression: {
		anchor:      "#\(name)"
		name:        string
		title:       string
		description: string
		return:      string

		grammar?: #Grammar

		examples: [remap.#Example, ...remap.#Example]
	}

	#Grammar: {
		source: string
		definitions: [Name=string]: {
			name:        Name
			description: string
			characteristics?: [Name=string]: remap.#Characteristic
			enum?: #Enum
			examples?: [string, ...string]
		}
	}

	expressions: [Name=string]: #Expression & {
		name: Name
	}
}
