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
		_commands: roles._systemd_commands & {
			_config_path: paths.config
			install: #"""
				curl -1sLf \
				  'https://repositories.timber.io/public/vector/cfg/setup/bash.deb.sh' \
				  | sudo -E bash
				"""#
			uninstall: "sudo apt remove vector"
		}
		agent:      roles._journald_agent & {commands:    _commands}
		aggregator: roles._vector_aggregator & {commands: _commands}
	}
}
