package metadata

installation: _interfaces: apt: {
	title: "Apt"
	description: """
		Advanced Package Tool, or APT, is a free package manager that
		handles the installation and removal of software on Debian,
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
		_commands: {
			install: #"""
				curl -1sLf \
				  'https://repositories.timber.io/public/vector/cfg/setup/bash.deb.sh' \
				  | sudo -E bash
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
		}
		agent: commands: _commands & {
			variables: config: sources: in: type: components.sources.journald.type
		}
		aggregator: commands: _commands & {
			variables: config: sources: in: type: components.sources.vector.type
		}
	}
}
