package metadata

installation: {
	#Commands: {
		{[Name=string]: string | null}
	} & {
		_config_path: string | *null
		let ConfigPath = _config_path

		_shell: string | *null
		let Shell = _shell

		configure: string | null | *"none"
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
		role_implementations: {
			_systemd_commands: {
				logs:    "sudo journalctl -fu vector"
				reload:  "systemctl kill -s HUP --kill-who=main vector.service"
				restart: "sudo systemctl restart vector"
				start:   "sudo systemctl start vector"
				stop:    "sudo systemctl stop vector"
			}
		}
		role_implementations:  #RoleImplementations & {_shell: Shell}
		name:                  string
		package_manager_name?: string
		platform_name?:        string
		title:                 string
	}

	#Interfaces: [Name=string]: #Interface & {
		name: Name
	}

	#RoleImplementation: {
		_shell: string | *null
		let Shell = _shell
		commands:    #Commands & {_shell: Shell}
		description: string
		name:        string
		title:       string
		tutorials:   #Tutorials
		variables:   #Variables
	}

	#RoleImplementations: [Name=string]: #RoleImplementation & {
		name: Name
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

			sources: [Name=string]: {
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

	_interfaces: #Interfaces
}
