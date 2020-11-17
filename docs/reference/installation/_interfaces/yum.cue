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
		_commands: roles._systemd_commands & {
			_config_path: paths.config
			install: #"""
				curl -1sLf \
				  'https://repositories.timber.io/public/vector/cfg/setup/bash.rpm.sh' \
				  | sudo -E bash
				"""#
			uninstall: "sudo yum remove vector"
		}
		agent:      roles._journald_agent & {commands:    _commands}
		aggregator: roles._vector_aggregator & {commands: _commands}
	}
}
