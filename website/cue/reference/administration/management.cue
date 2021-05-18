package metadata

administration: management: {
	#Interface: {
		#Command: {
			command: string
			info?: string
		}

		name:  string
		title: string | *name

		variables: {
			variants?: [string, ...string]

			config_formats: ["toml", "yaml", "json"]
		}

		manage?: {
			start?:   #Command
			stop?:    #Command
			reload?:  #Command
			restart?: #Command
		}

		observe?: {
			logs?:    #Command
			metrics?: #Command
		}
	}

	#Interfaces: [Name=string]: #Interface & {name: Name}

	_interfaces: #Interfaces & {
		_systemd: {
			manage: {
				start:   {
					command: "sudo systemctl start vector"
				}
				stop:    {
					command: "sudo systemctl stop vector"
				}
				reload:  {
					command: "systemctl kill -s HUP --kill-who=main vector.service"
				}
				restart: {
					command: "sudo systemctl restart vector"
				}
			}

			observe: {
				logs: {
					command: "sudo journalctl -fu vector"
				}
			}
		}

		apt: _systemd & {
			title: "APT"
		}

		docker_cli: {
			title: "Docker CLI"

			variables: {
				variants: ["alpine", "debian", "distroless"]
			}

			manage: {
				start: {
					command: #"""
						docker run \
						  -d \
						  -v ~/vector.{config_format}:/etc/vector/vector.{config_format}:ro \
						  -p 8686:8686 \
						  timberio/vector:{version}-{variant}
						"""#
				}
				stop:    {
					command: "docker stop timberio/vector"
				}
				reload:  {
					command: "docker kill --signal=HUP timberio/vector"
				}
				restart: {
					command: "docker restart -f $(docker ps -aqf \"name=vector\")"
				}
			}

			observe: {
				logs: {
					command: "docker logs -f $(docker ps -aqf \"name=vector\")"
				}
			}
		}

		dpkg: _systemd

		homebrew: {
			title: "Homebrew"

			manage: {
				start: {
					command: "brew services start vector"
				}
				stop: {
					command: "brew services stop vector"
				}
				reload: {
					command: "killall -s SIGHUP vector"
				}
				restart: {
					command: "brew services restart vector"
				}
			}

			observe: {
				logs: {
					command:  "tail -f /usr/local/var/log/vector.log"
				}
			}
		}

		msi: {
			title: "MSI"

			manage: {
				start: {
					command: #"""
						C:\Program Files\Vector\bin\vector \
						  --config C:\Program Files\Vector\config\vector.{config_format}
						"""#
				}
			}
		}

		nix: {
			title: "Nix"

			manage: {
				start: {
					command: "vector --config /etc/vector/vector.{config_format}"
				}

				reload: {
					command: "killall -s SIGHUP vector"
				}
			}
		}

		rpm: _systemd & {
			title: "RPM"
		}

		vector_installer: {
			title: "Vector Installer"

			manage: {
				start: {
					command: "vector --config /etc/vector.{config_format}"
				}

				reload: {
					command: "killall -s SIGHUP vector"
				}
			}
		}

		yum: _systemd & {
			title: "YUM"
		}
	}
}
