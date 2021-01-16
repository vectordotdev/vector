package metadata

remap2: concepts: {
	#Concept: {
		name:        string
		title:       string
		description: string
	}

	{[Name=string]: #Concept & {
		name: Name
	}}
}
