package metadata

// These CUE sources are the beginnings of an effort to re-architect some of the administration sources from a UI-first
// perspective.

administration: management: {
	#Interface: {
		#Command: {
			command?: string
			info?:    string
		}

		name:  string
		title: string | *name

		variables: {
			variants?: [string, ...string]

			config_formats: ["yaml", "toml", "json"]
		}

		manage?: {
			start?:   #Command
			stop?:    #Command
			reload?:  #Command
			restart?: #Command
			upgrade?: #Command
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
				start: {
					command: "sudo systemctl start vector"
				}
				stop: {
					command: "sudo systemctl stop vector"
				}
				reload: {
					command: "systemctl kill -s HUP --kill-who=main vector.service"
				}
				restart: {
					command: "sudo systemctl restart vector"
				}
			}
		}

		apt: _systemd & {
			title: "APT"

			manage: {
				upgrade: {
					command: "sudo apt-get upgrade vector"
				}
			}

			observe: {
				logs: {
					info: """
						The Vector package from the APT repository installs Vector as a [systemd](\(urls.systemd))
						service. You can access Vector's logs using the [`journalctl`](\(urls.journalctl)) utility:

						```bash
						sudo journalctl -fu vector
						```
						"""
				}
			}
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
				stop: {
					command: "docker stop timberio/vector"
				}
				reload: {
					command: "docker kill --signal=HUP timberio/vector"
				}
				restart: {
					command: "docker restart -f $(docker ps -aqf \"name=vector\")"
				}
			}

			observe: {
				logs: {
					info: """
						If you've started Vector with the `docker` CLI you can access Vector's logs using the
						`docker logs` command. First, find the Vector container ID:

						```bash
						docker ps | grep vector
						```

						Then copy Vector's container ID and use it to tail the logs:

						```bash
						docker logs -f <container-id>
						```

						If you started Vector with the [Docker Compose](\(urls.docker_compose)) CLI you can use this
						command to access Vector's logs:

						```bash
						docker-compose logs -f vector
						```

						Replace `vector` with the name of Vector's service if you've named it something else.
						"""
				}
			}
		}

		dpkg: _systemd & {
			observe: {
				logs: {
					info: """
						The Vector DEB package installs Vector as a Systemd service. Logs can be accessed using the
						`journalctl` utility:

						```bash
						sudo journalctl -fu vector
						```
						"""
				}
			}
		}

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
				upgrade: {
					command: "brew update && brew upgrade vector"
				}
			}

			observe: {
				logs: {
					info: """
						When Vector is started through [Homebrew](\(urls.homebrew)) the logs are automatically routed to
						`/usr/local/var/log/vector.log`. You can tail them using the `tail` utility:

						```bash
						tail -f /usr/local/var/log/vector.log
						```
						"""
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

			observe: {
				logs: {
					info: """
						The Vector MSI package doesn't install Vector into a process manager. Therefore, you need to
						start Vector by executing the Vector binary directly. Vector's logs are written to `STDOUT`. You
						are in charge of routing `STDOUT`, and this determines how you access Vector's logs.
						"""
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

				upgrade: {
					command: #"""
						nix-env \
						  --file https://github.com/NixOS/nixpkgs/archive/master.tar.gz \
						  --upgrade vector
						"""#
				}
			}

			observe: {
				logs: {
					info: """
						The Vector Nix package doesn't install Vector into a process manager. Therefore, Vector must be
						started by executing the Vector binary directly. Vector's logs are written to `STDOUT`. You are
						in charge of routing `STDOUT`, and this determines how you access Vector's logs.
						"""
				}
			}
		}

		rpm: _systemd & {
			title: "RPM"

			observe: {
				logs: {
					info: """
						The Vector RPM package installs Vector as a Systemd service.  You can access Vector's logs using
						the [`journalctl`](\(urls.journalctl)) utility:

						```bash
						sudo journalctl -fu vector
						```
						"""
				}
			}
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

			manage: {
				upgrade: {
					command: "sudo yum upgrade vector"
				}
			}
		}
	}
}
