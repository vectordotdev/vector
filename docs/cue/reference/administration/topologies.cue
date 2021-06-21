package metadata

administration: {
	#Topology: {
		#Attribute: {
			title:       string
			description: string
		}

		name:        string
		title:       string
		description: string
		// Enables the topologies to be displayed in a specified order rather than alphabetically
		order: int

		pros: [#Attribute, ...#Attribute]
		cons: [#Attribute, ...#Attribute]
	}

	#Topologies: [Name=string]: #Topology & {
		name: Name
	}

	topologies: #Topologies
}
