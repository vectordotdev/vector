package metadata

remap: {
	#Feature: {
		anchor:      name
		name:        string
		title:       string
		description: string

		principles: {
			for k, v in remap.principles {
				"\( k )": bool
			}
		}

		characteristics: remap.#Characteristics
	}

	features: [Name=string]: #Feature & {
		name: Name
	}
}
