package metadata

installation: _interfaces: dpkg: {
	archs: ["x86_64", "ARM64", "ARMv7"]
	roles: {
		_commands: {
			_config_path: "/etc/vector/vector.{config_format}"
			install: #"""
				curl --proto '=https' --tlsv1.2 -O https://packages.timber.io/vector/{version}/vector-{arch}.deb && \
					sudo dpkg -i vector-{arch}.deb
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
			variables: {
				arch: ["amd64", "arm64", "armhf"]
				version: true
			}
		}
		agent: commands: _commands & {
			variables: config: sources: in: type: components.sources.journald.type
		}
		sidecar: commands:    _commands
		aggregator: commands: _commands
	}
	package_manager_name: installation.package_managers.dpkg.name
	title:                "DPKG"
}
