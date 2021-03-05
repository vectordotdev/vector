package metadata

installation: _interfaces: apt: {
	title:       "Apt"
	description: """
		[Advanced Package Tool](\(urls.apt)), or APT, is a free package manager
		that handles the installation and removal of software on Debian,
		Ubuntu, and other Linux distributions.

		Our APT repositories are provided by
		[Cloudsmith](\(urls.cloudsmith)) and you
		can find [instructions for manually adding
		the repositories](\(urls.cloudsmith_apt)).
		"""

	archs: ["x86_64", "ARM64", "ARMv7"]
	package_manager_name: installation.package_managers.apt.name
	paths: {
		bin:         "/usr/bin/vector"
		bin_in_path: true
		config:      "/etc/vector/vector.{config_format}"
	}

	role_implementations: [string]: {
		commands: role_implementations._systemd_commands & {
			_config_path: paths.config
			add_repo:
				#"""
					curl -1sLf \
					  'https://repositories.timber.io/public/vector/cfg/setup/bash.deb.sh' \
					  | sudo -E bash
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
