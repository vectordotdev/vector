package metadata

installation: _interfaces: homebrew: {
	archs: ["x86_64", "ARM64", "ARMv7"]
	roles: {
		_commands: {
			_config_path: "/etc/vector/vector.{config_format}"
			install: #"""
				curl -1sLf \
				  'https://repositories.timber.io/public/vector/cfg/setup/bash.deb.sh' \
				  | sudo -E bash
				"""#
			configure: #"""
				cat <<-VECTORCFG > \#(_config_path)
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
			variables: config: sources: in: type: components.sources.file.type
		}
		sidecar: commands:    _commands
		aggregator: commands: _commands
	}
	package_manager_name: installation.package_managers.homebrew.name
	title:                "Homebrew"
}
