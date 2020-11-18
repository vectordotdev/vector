package metadata

installation: _interfaces: apt: {
	title:       "Apt"
	description: """
		[Advanced Package Tool](\(urls.apt)), or APT, is a free package manager
		that handles the installation and removal of software on Debian,
		Ubuntu, and other Linux distributions.
		"""

	archs: ["x86_64", "ARM64", "ARMv7"]
	package_manager_name: installation.package_managers.apt.name
	paths: {
		bin:         "/usr/bin/vector"
		bin_in_path: true
		config:      "/etc/vector/vector.{config_format}"
	}
	roles: {
		_commands: roles._systemd_commands & roles._bash_configure & {
			_config_path: paths.config
			install:
				#"""
					curl -1sLf \
					  'https://repositories.timber.io/public/vector/cfg/setup/bash.deb.sh' \
					  | sudo -E bash && \
					  sudo apt-get install vector
					"""#
			uninstall: "sudo apt remove vector"
			upgrade:   "sudo apt-get upgrade vector"
		}
		_tutorials: {
			_commands: _
			installation: [
				{
					title:   "Install Vector"
					command: _commands.install
				},
				{
					title:   "Configure Vector"
					command: _commands.configure
				},
				{
					title:   "Restart Vector"
					command: _commands.restart
				},
			]
		}
		agent: roles._journald_agent & {
			commands:  _commands
			tutorials: _tutorials & {_commands: commands}
		}
		aggregator: roles._vector_aggregator & {
			commands:  _commands
			tutorials: _tutorials & {_commands: commands}
		}
	}
}
