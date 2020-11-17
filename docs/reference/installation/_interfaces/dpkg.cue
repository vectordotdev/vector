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
		_commands: roles._systemd_commands & {
			_config_path: paths.config,
			install: #"""
				curl --proto '=https' --tlsv1.2 -O https://packages.timber.io/vector/{version}/vector-{arch}.deb && \
					sudo dpkg -i vector-{arch}.deb
				"""#
			uninstall: "sudo dpkg -r vector"
			variables: {
				arch: ["amd64", "arm64", "armhf"]
				version: true
			}
		}
		agent:      roles._journald_agent & {commands:    _commands}
		aggregator: roles._vector_aggregator & {commands: _commands}
	}
	package_manager_name: installation.package_managers.dpkg.name
	title:                "DPKG"
}
