package metadata

installation: _interfaces: nix: {
	archs: ["x86_64", "ARM64", "ARMv7"]
	roles: {
		_commands: {
			_config_path: "/etc/vector/vector.{config_format}"
			install: #"""
				nix-env --file https://github.com/NixOS/nixpkgs/archive/master.tar.gz --install --attr vector
				"""#
			configure: #"""
				cat <<-VECTORCFG > \#(_config_path)
				{config}
				VECTORCFG
				"""#
			start:     #"""
				vector --config \#(_config_path)
				"""#
			stop:      null
			reload: #"""
				ps axf | grep vector | grep -v grep | awk '{print "kill -SIGHUP " $1}' | sh
				"""#
			logs: null
		}
		agent: commands: _commands & {
			variables: config: sources: in: type: components.sources.journald.type
		}
		sidecar: commands:    _commands
		aggregator: commands: _commands
	}
	package_manager_name: installation.package_managers.nix.name
	title:                "Nix"
}
