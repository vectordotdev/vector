package metadata

installation: _interfaces: nix: {
	title:       "Nix"
	description: """
				[Nix](\(urls.nix)) is a cross-platform package manager
				implemented on a functional deployment model where software is
				installed into unique directories generated through
				cryptographic hashes, it is also the name of the programming
				language.
				"""

	archs: ["x86_64", "ARM64", "ARMv7"]
	package_manager_name: installation.package_managers.nix.name
	paths: {
		bin:         "/usr/bin/vector"
		bin_in_path: true
		config:      "/etc/vector/vector.{config_format}"
	}
	roles: {
		_commands: {
			install: #"""
				nix-env --file https://github.com/NixOS/nixpkgs/archive/master.tar.gz --install --attr vector
				"""#
			configure: #"""
				cat <<-VECTORCFG > \#(paths.config)
				{config}
				VECTORCFG
				"""#
			start:     #"""
				vector --config \#(paths.config)
				"""#
			stop:      null
			reload: #"""
				ps axf | grep vector | grep -v grep | awk '{print "kill -SIGHUP " $1}' | sh
				"""#
			logs: null
		}
		agent:      roles._journald_agent & {commands:    _commands}
		aggregator: roles._vector_aggregator & {commands: _commands}
	}
}
