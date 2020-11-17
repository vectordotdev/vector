package metadata

installation: _interfaces: rpm: {
	title:       "RPM"
	description: """
		[RPM Package Manager](\(urls.rpm)) is a free and open-source package
		management system for installing and managing software on Fedra, CentOS,
		OpenSUSE, OpenMandriva, Red Hat Enterprise Linux, and other
		related Linux-based systems.
		"""

	archs: ["x86_64", "ARM64", "ARMv7"]
	package_manager_name: installation.package_managers.rpm.name
	paths: {
		bin:         "/usr/bin/vector"
		bin_in_path: true
		config:      "/etc/vector/vector.{config_format}"
	}
	roles: {
		_commands: roles._systemd_commands & roles._bash_configure & {
			_config_path: paths.config
			install:      "sudo rpm -i https://packages.timber.io/vector/{version}/vector-{arch}.rpm"
			uninstall:    "sudo rpm -e vector"
			upgrade:      null
			variables: {
				arch: ["x86_64", "aarch64", "armv7"]
				version: true
			}
		}
		agent:      roles._journald_agent & {commands:    _commands}
		aggregator: roles._vector_aggregator & {commands: _commands}
	}
}
