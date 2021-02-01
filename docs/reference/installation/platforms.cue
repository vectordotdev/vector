package metadata

installation: {
	#Platform: {
		description:               string
		how_it_works:              #HowItWorks
		minimum_supported_version: string | null
		name:                      string
		title:                     string
	}

	#Platforms: [Name=string]: #Platform & {
		name: Name
	}

	platforms: #Platforms
}
