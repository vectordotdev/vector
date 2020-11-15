package metadata

installation: _interfaces: dpkg: {
	description: """
		Dpkg is the software that powers the package management system
		in the Debian operating system and its derivatives. Dpkg is used
		to install and manage software via `.deb` packages.
		"""

	archs: ["x86_64", "ARM64", "ARMv7"]
	paths: {
		bin:         "/usr/bin/vector"
		bin_in_path: true
		config:      "/etc/vector/vector.{config_format}"
	}
	roles: {
		_commands: {
			install: #"""
				curl --proto '=https' --tlsv1.2 -O https://packages.timber.io/vector/{version}/vector-{arch}.deb && \
					sudo dpkg -i vector-{arch}.deb
				"""#
			configure: #"""
				cat <<-VECTORCFG > \#(paths.config)
				{config}
				VECTORCFG
				"""#
			start: #"""
				sudo systemctl start vector
				"""#
			stop: #"""
				sudo systemctl stop vector
				"""#
			reload: #"""
				systemctl kill -s HUP --kill-who=main vector.service
				"""#
			logs: #"""
				sudo journalctl -fu vector
				"""#
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
