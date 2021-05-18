package metadata

installation: {
	#OperatingSystem: {
		description: string
		family:      #OperatingSystemFamily
		interfaces: [installation.#Interface & {_shell: shell}, ...installation.#Interface & {_shell: shell}]
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
