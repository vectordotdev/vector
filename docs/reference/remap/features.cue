package metadata

remap: {
	#Feature: {
		anchor:      "#\(name)"
		name:        string
		title:       string
		description: string

		characteristics: [Name=string]: remap.#Characteristic
	}

	features: [Name=string]: #Feature & {
		name: Name
	}
}
