package metadata

remap2: {
	#Expression: {
		anchor:      "#\(name)"
		name:        string
		title:       string
		description: string
		return:      string

		grammar?: #Grammar

		examples?: [remap2.#Example, ...remap2.#Example]
	}

	#Grammar: {
		source: string
		definitions: [Name=string]: {
			description: string
			characteristics: [Name=string]: remap2.#Characteristic
			enum?: #Enum
			examples?: [string, ...string]
		}
	}

	expressions: [Name=string]: #Expression & {
		name: Name
	}
}
