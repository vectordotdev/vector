package metadata

administration: {
	#Role: {
		name:         string
		title:        string
		description?: string
		sub_roles: [SubName=string]: {
			name:        SubName
			title:       string
			description: string
		}
	}

	#Roles: [Name=string]: #Role & {
		name: Name
	}

	roles: #Roles
}
