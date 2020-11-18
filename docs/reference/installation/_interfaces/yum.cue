package metadata

installation: _interfaces: yum: {
	title:       "YUM"
	description: """
		The [Yellowdog Updater](\(urls.yum)), Modified (YUM) is a free and
		open-source command-line package-manager for Linux operating system
		using the RPM Package Manager.
		"""

	archs: ["x86_64", "ARM64", "ARMv7"]
	package_manager_name: installation.package_managers.yum.name
	paths: {
		bin:         "/usr/bin/vector"
		bin_in_path: true
		config:      "/etc/vector/vector.{config_format}"
	}
	roles: {
		_commands: roles._systemd_commands & roles._bash_configure & {
			_config_path: paths.config
			install: #"""
				curl -1sLf \
				  'https://repositories.timber.io/public/vector/cfg/setup/bash.rpm.sh' \
				  | sudo -E bash && \
				  sudo yum install vector
				"""#
			uninstall: "sudo yum remove vector"
			upgrade:   "sudo yum upgrade vector"
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
