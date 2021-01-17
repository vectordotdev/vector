package metadata

remap: expressions: {
	#Expression: {
		anchor:      "#\(name)"
		name:        string
		title:       string
		description: string
		return:      string

		grammar?: #Grammar

		examples?: [remap.#Example, ...remap.#Example]
	}

	#Grammar: {
		source: string
		definitions: [Name=string]: {
			description: string
			characteristics: [Name=string]: remap.#Characteristic
			enum?: #Enum
			examples?: [string, ...string]
		}
	}

	{[Name=string]: #Expression & {
		name: Name
	}}
}
