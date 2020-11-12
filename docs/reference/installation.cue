package metadata

installation: close({
	#Commands: {
		install:   string | null
		configure: string
		start:     string
		stop:      string | null
		reload:    string | null
		logs:      string | null
		variables: {
			arch?: [string, ...string]
			flags?: {
				sources?:    _
				transforms?: _
				sinks?:      _
			}
			config: {
				sources: in: {
					type: string

					if type == "file" {
						include: [string, ...string]
					}
				}

				sinks: out: {
					type: "console"
					inputs: ["in"]
				}
			}
			config_format: ["toml", "yaml", "json"]
			variant?: [string, ...string]
			version: bool | *false
		}
	}

	#Downloads: [Name=string]: {
		available_on_latest:  bool
		available_on_nightly: bool
		arch:                 #Arch
		file_name:            string
		file_type:            string
		os:                   #OperatingSystem
		package_manager?:     string
		title:                "\(os) (\(arch))"
		type:                 "archive" | "package"
	}

	#Interface: {
		archs: [#Arch, ...#Arch]
		roles: {
			agent: commands:   #Commands
			sidecar: commands: #Commands & {
				variables: config: {
					sources: in: {
						type:    components.sources.file.type
						include: [string, ...string] | *[components.sources.file.configuration.include.type.array.items.type.string.examples[0]]
					}
				}
			}
			aggregator: commands: #Commands & {
				variables: config: sources: in: type: components.sources.vector.type
			}
		}
		name:                  string
		package_manager_name?: string
		platform_name?:        string
		title:                 string
	}

	#Interfaces: [Name=string]: #Interface & {
		name: Name
	}

	#OperatingSystems: [Name=string]: {
		interfaces: [#Interface, ...#Interface]
		name:  Name
		os:    string
		title: string
	}

	#PackageManagers: [Name=string]: {
		name:  Name
		title: string
	}

	#Platforms: [Name=string]: {
		description: string
		name:        Name
		title:       string
	}

	#Roles: [Name=string]: {
		name:  Name
		title: string
	}

	_interfaces:       #Interfaces
	downloads:         #Downloads
	operating_systems: #OperatingSystems
	package_managers:  #PackageManagers
	platforms:         #Platforms
	roles:             #Roles
})
