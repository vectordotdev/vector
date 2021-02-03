package metadata

remap: {
	#Principle: {
		anchor:      name
		name:        string
		title:       string
		description: string
	}

	principles: [Name=string]: #Principle & {
		name: Name
	}
}
