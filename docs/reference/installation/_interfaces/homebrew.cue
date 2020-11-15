package metadata

installation: _interfaces: homebrew: {
	title: "Homebrew"
	description: """
		Homebrew is a free and open-source package management system
		that manage software installation and management for Apple's
		MacOS operating system and other supported Linux systems.
		"""

	archs: ["x86_64", "ARM64", "ARMv7"]
	package_manager_name: installation.package_managers.homebrew.name
	paths: {
		bin:         "/usr/local/bin/vector"
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
			variables: config: sources: in: {
				type:    components.sources.file.type
				include: [string, ...string] | *["/var/log/system.log"]
			}
		}
		aggregator: commands: _commands & {
			variables: config: sources: in: type: components.sources.vector.type
		}
	}
}
