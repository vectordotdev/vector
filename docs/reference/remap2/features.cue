package metadata

remap2: {
	#Feature: {
		name: string
		title: string
		description: string

		characteristics: [Name=string]: remap2.#Characteristic & {
			name: Name
		}
	}

	features: [Name=string]: #Feature & {
		name: Name
	}
}
