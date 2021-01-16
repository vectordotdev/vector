package metadata

remap2: {
	#Construct: {
		anchor: "#\(name)"
		name: string
		title: string
		description: string
		examples?: [string, ...string]

		characteristics: [Name=string]: remap2.#Characteristic & {
			name: Name
		}

		constructs: [Name=string]: #Construct & {
			name: Name
		}
	}

	constructs: [Name=string]: #Construct & {
		name: Name
	}
}
