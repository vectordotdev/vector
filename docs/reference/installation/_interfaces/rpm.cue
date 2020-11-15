package metadata

installation: _interfaces: rpm: {
	title: "RPM"
	description: """
		RPM Package Manager is a free and open-source package management
		system for installing and managing software on Fedra, CentOS,
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
		_commands: {
			configure: #"""
				cat <<-VECTORCFG > \#(paths.config)
				{config}
				VECTORCFG
				"""#
			install: #"""
				sudo rpm -i https://packages.timber.io/vector/{version}/vector-{arch}.rpm
				"""#
			logs: #"""
				sudo journalctl -fu vector
				"""#
			reload: #"""
				systemctl kill -s HUP --kill-who=main vector.service
				"""#
			start: #"""
				sudo systemctl start vector
				"""#
			stop: #"""
				sudo systemctl stop vector
				"""#
			uninstall: #"""
				sudo rpm -e vector
				"""#
			variables: {
				arch: ["x86_64", "aarch64", "armv7"]
				version: true
			}
		}
		agent:      roles._journald_agent & {commands:    _commands}
		aggregator: roles._vector_aggregator & {commands: _commands}
	}
}
