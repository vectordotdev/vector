package metadata

administration: interfaces: apt: {
	title:       "Apt"
	description: """
		[Advanced Package Tool](\(urls.apt)), or APT, is a free package manager that handles the
		installation and removal of software on [Debian](\(urls.debian)), [Ubuntu](\(urls.ubuntu)),
		and other Linux distributions.

		Our APT repositories are provided by [Datadog](\(urls.datadog)).
		"""

	archs: ["x86_64", "ARM64", "ARMv7"]
	package_manager_name: administration.package_managers.apt.name
	paths: {
		bin:         "/usr/bin/vector"
		bin_in_path: true
		config:      "/etc/vector/vector.{config_format}"
	}

	role_implementations: [string]: {
		commands: role_implementations._systemd_commands & {
			add_repo:
				#"""
					# One of the following:

					# Use repository installation script
					curl -1sLf \
					  'https://repositories.timber.io/public/vector/cfg/setup/bash.deb.sh' \
					  | sudo -E bash

					# Use extrepo
					sudo apt install extrepo
					sudo extrepo enable vector
					"""#
			install:   "sudo apt-get install vector"
			uninstall: "sudo apt remove vector"
			upgrade:   "sudo apt-get upgrade vector"
		}
		tutorials: {
			installation: [
				{
					title:   "Add the Vector repo"
					command: commands.add_repo
				},
				{
					title:   "Install Vector"
					command: commands.install
				},
				{
					title:   "Configure Vector"
					command: commands.configure
				},
				{
					title:   "Restart Vector"
					command: commands.restart
				},
			]
		}
		variables: {}
	}

	role_implementations: {
		agent:      role_implementations._journald_agent
		aggregator: role_implementations._vector_aggregator
	}
}
