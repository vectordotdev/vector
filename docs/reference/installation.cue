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
				sources: [Name=string]: {
					type: string

					include?: [string, ...string]
				}

				sinks: [Name=string]: {
					type: "console"
					inputs: ["in"]
				}
			}
			config_format: ["toml"]
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
		description: string
		paths: {
			bin:         string
			bin_in_path: bool
			config:      string
		}
		roles: {
			_file_agent: {
				commands: variables: config: sources: {
					in_logs: {
						type:    components.sources.file.type
						include: [string, ...string] | *["/var/log/**/*.log"]
					}
					in_metrics: type: components.sources.host_metrics.type
				}
				description: #"""
							The agent role is designed to collect all data on
							a single host. Vector runs as a background process
							and interfaces with a host-level APIs for data
							collection. By default, Vector will collect logs
							via Vector's [`file` source](\#(urls.vector_journald_source)) and
							metrics via the [`host_metrics` source](\#(urls.vector_host_metrics_source)),
							but it is recommended to adjust your pipeline as
							necessary using Vector's [sources](\#(urls.vector_sources)),
							[transforms](\#(urls.vector_transforms)), and
							[sinks](\#(urls.vector_sinks)).
							"""#
				title:       "Agent"
			}
			_file_sidecar: {
				commands: variables: config: sources: {
					in_logs: {
						type:    components.sources.file.type
						include: [string, ...string] | *["/var/log/my-app*.log"]
					}
					in_metrics: type: components.sources.host_metrics.type
				}
				description: #"""
							The sidecar role is designed to collect data from
							a single process on the same host. By default, we
							recommend using the [`file` source](\#(urls.vector_file_source))
							to tail the logs for that individual process, but
							you could use the [`stdin` source](\#(urls.vector_stdin_source)),
							[`socket` source](\#(urls.vector_socket_source)), or
							[`http` source](\#(urls.vector_http_source)). We recommend
							adjusting your pipeline as necessary using Vector's
							[sources](\#(urls.vector_sources)),
							[transforms](\#(urls.vector_transforms)), and
							[sinks](\#(urls.vector_sinks)).
							"""#
				title:       "Sidecar"
			}
			_journald_agent: {
				commands: variables: config: sources: {
					in_logs: type:    components.sources.journald.type
					in_metrics: type: components.sources.host_metrics.type
				}
				description: #"""
							The agent role is designed to collect all data on
							a single host. Vector runs as a background process
							and interfaces with a host-level APIs for data
							collection. By default, Vector will collect logs
							from [Journald](\#(urls.journald)) via Vector's
							[`journald` source](\#(urls.vector_journald_source)) and
							metrics via the [`host_metrics` source](\#(urls.vector_host_metrics_source)),
							but it is recommended to adjust your pipeline as
							necessary using Vector's [sources](\#(urls.vector_sources)),
							[transforms](\#(urls.vector_transforms)), and
							[sinks](\#(urls.vector_sinks)).
							"""#
				title:       "Agent"
			}
			_vector_aggregator: {
				commands: variables: config: sources: in: type: components.sources.vector.type
				description: #"""
							The aggregator role is designed to receive and
							process data from multiple upstream sources.
							Typically these are other Vector agents, but it
							could be anything, including non-Vector agents.
							By default, we recommend the [`vector` source](\#(urls.vector_source))
							since it supports all data types, but it is
							recommended to adjust your pipeline as necessary
							using Vector's [sources](\#(urls.vector_sources)),
							[transforms](\#(urls.vector_transforms)), and
							[sinks](\#(urls.vector_sinks)).
							"""#
				title:       "Aggregator"
			}
		}
		roles: [Name=string]: {
			commands:    #Commands
			description: string
			name:        Name
			title:       string
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
		description: string
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
