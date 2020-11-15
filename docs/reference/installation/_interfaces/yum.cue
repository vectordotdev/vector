package metadata

installation: _interfaces: yum: {
	title: "YUM"
	description: """
		The Yellowdog Updater, Modified (YUM) is a free and open-source
		command-line package-manager for Linux operating system using
		the RPM Package Manager.
		"""

	archs: ["x86_64", "ARM64", "ARMv7"]
	package_manager_name: installation.package_managers.yum.name
	paths: {
		bin:         "/usr/bin/vector"
		bin_in_path: true
		config:      "/etc/vector/vector.{config_format}"
	}
	roles: {
		_commands: {
			install: #"""
				curl -1sLf \
				  'https://repositories.timber.io/public/vector/cfg/setup/bash.rpm.sh' \
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
