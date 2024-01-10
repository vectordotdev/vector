package metadata

administration: {
	#Interface: {
		_shell: "powershell" | *"bash"

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
		role_implementations:  #RoleImplementations
		name:                  string
		package_manager_name?: string
		platform_name?:        string
		title:                 string

		#RoleImplementation: {
			commands:    #Commands
			description: string
			name:        string
			title:       string
			tutorials:   #Tutorials
			variables:   #Variables
		}

		#RoleImplementations: [Name=string]: #RoleImplementation & {
			name: Name
		}

		#Commands: {
			{[Name=string]: string | null}
		} & {
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

			if _shell == "bash" {
				configure: string | *#"""
					cat <<-'VECTORCFG' > \#(paths.config)
					{config}
					VECTORCFG
					"""#
			}

			if _shell == "powershell" {
				configure: string | *#"""
					@"
					{config}
					"@ | Out-File -FilePath \#(paths.config)
					"""#
			}
		}

	}

	#Interfaces: [Name=string]: #Interface & {
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
				type: "console"
				inputs: [string, ...string] | *[for id, _source in sources {id}]
				encoding: codec: "json"
			}
		}
		config_format: ["toml"]
		variant?: [string, ...string]
		version: bool | *false
	}

	interfaces: #Interfaces
}
