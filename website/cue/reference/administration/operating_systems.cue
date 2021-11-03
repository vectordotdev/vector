package metadata

administration: {
	#OperatingSystem: {
		description: string
		family:      #OperatingSystemFamily
		interfaces: [administration.#Interface & {_shell: shell}, ...administration.#Interface & {_shell: shell}]
		minimum_supported_version: string | null
		name:                      string
		shell:                     string
		title:                     string
	}

	#OperatingSystems: [Name=string]: #OperatingSystem & {
		name: Name
	}

	operating_systems: #OperatingSystems
}
