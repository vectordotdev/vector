package metadata

installation: close({
	#Commands: {
		{[Name=string]: string | null}
	} & {
		_config_path: string | *null
		let ConfigPath = _config_path

		_shell: string | *null
		let Shell = _shell

		configure: string | null
		install:   string | null
		logs:      string | null
		reload:    string | null
		restart:   string | null
		start:     string | null
		stop:      string | null
		top:       string | null | *"vector top"
		uninstall: string
		upgrade:   string | null

		if Shell == "bash" {
			configure: string | *#"""
					cat <<-'VECTORCFG' > \#(ConfigPath)
					{config}
					VECTORCFG
					"""#
		}

		if Shell == "powershell" {
			configure: string | *#"""
					@"
					{config}
					"@ | Out-File -FilePath \#(ConfigPath)
					"""#
		}
	}

	#Downloads: [Name=string]: {
		available_on_latest:  bool
		available_on_nightly: bool
		arch:                 #Arch
		file_name:            string
		file_type:            string
		os:                   #OperatingSystemFamily
		package_manager?:     string
		title:                "\(os) (\(arch))"
		type:                 "archive" | "package"
	}

	#Interface: {
		_shell: string | *null
		let Shell = _shell

		archs: [#Arch, ...#Arch]
		description: string
		paths: {
			bin:         string | null
			bin_in_path: bool | null
			config:      string | null
		}
		roles: {
			_file_agent: {
				variables: config: {
					sources: {
						logs: {
							type:    components.sources.file.type
							include: [string, ...string] | *["/var/log/**/*.log"]
						}
						host_metrics: type:     components.sources.host_metrics.type
						internal_metrics: type: components.sources.internal_metrics.type
					}
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
				variables: config: {
					sources: {
						logs: {
							type:    components.sources.file.type
							include: [string, ...string] | *["/var/log/my-app*.log"]
						}
						host_metrics: type:     components.sources.host_metrics.type
						internal_metrics: type: components.sources.internal_metrics.type
					}
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
				variables: config: {
					sources: {
						logs: type:             components.sources.journald.type
						host_metrics: type:     components.sources.host_metrics.type
						internal_metrics: type: components.sources.internal_metrics.type
					}
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
			_systemd_commands: {
				logs:    "sudo journalctl -fu vector"
				reload:  "systemctl kill -s HUP --kill-who=main vector.service"
				restart: "sudo systemctl restart vector"
				start:   "sudo systemctl start vector"
				stop:    "sudo systemctl stop vector"
			}
			_vector_aggregator: {
				variables: config: {
					sources: {
						vector: type:           components.sources.vector.type
						internal_metrics: type: components.sources.internal_metrics.type
					}
				}
				description: #"""
							The aggregator role is designed to receive and
							process data from multiple upstream agents.
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
			commands:    #Commands & {_shell: Shell}
			description: string
			name:        Name
			title:       string
			tutorials:   #Tutorials
			variables:   #Variables
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
		family:      #OperatingSystemFamily
		interfaces: [#Interface & {_shell: shell}, ...#Interface & {_shell: shell}]
		minimum_supported_version: string | null
		name:                      Name
		shell:                     string
		title:                     string
	}

	#PackageManagers: [Name=string]: {
		description: string
		name:        Name
		title:       string
	}

	#Platforms: [Name=string]: {
		description:               string
		how_it_works:              #HowItWorks
		minimum_supported_version: string | null
		name:                      Name
		title:                     string
	}

	#Roles: [Name=string]: {
		name:         Name
		title:        string
		description?: string
		sub_roles: [SubName=string]: {
			name:        SubName
			title:       string
			description: string
		}
	}

	#Tutorials: {
		installation: [...{
			title:   string
			command: string
		}]
	}

	#Variables: {
		arch?: [string, ...string]
		flags?: {
			sources?:    _
			transforms?: _
			sinks?:      _
		}
		config: {
			api: {
				enabled: true
				address: "127.0.0.1:8686"
			}

			sources?: [Name=string]: {
				type: string

				if type == "file" {
					include: [string, ...string]
				}
			}

			sinks: out: {
				type:   "console"
				inputs: [string, ...string] | *[ for id, _source in sources {id}]
				encoding: codec: "json"
			}
		}
		config_format: ["toml"]
		variant?: [string, ...string]
		version: bool | *false
	}

	_interfaces:       #Interfaces
	downloads:         #Downloads
	operating_systems: #OperatingSystems
	package_managers:  #PackageManagers
	platforms:         #Platforms
	roles:             #Roles
})
