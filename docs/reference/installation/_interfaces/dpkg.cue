package metadata

installation: _interfaces: dpkg: {
	description: """
		[Dpkg](\(urls.dpkg)) is the software that powers the package management
		system in the Debian operating system and its derivatives. Dpkg is used
		to install and manage software via `.deb` packages.
		"""

	archs: ["x86_64", "ARM64", "ARMv7"]
	paths: {
		bin:         "/usr/bin/vector"
		bin_in_path: true
		config:      "/etc/vector/vector.{config_format}"
	}
	roles: {
		_commands: roles._systemd_commands & roles._bash_configure & {
			_config_path: paths.config
			install: #"""
				curl --proto '=https' --tlsv1.2 -O https://packages.timber.io/vector/{version}/vector-{arch}.deb && \
					sudo dpkg -i vector-{arch}.deb
				"""#
			uninstall: "sudo dpkg -r vector"
			upgrade:   null
			variables: {
				arch: ["amd64", "arm64", "armhf"]
				version: true
			}
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
	package_manager_name: installation.package_managers.dpkg.name
	title:                "DPKG"
}
