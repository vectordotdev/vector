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
		_commands: roles._bash_configure & {
			_config_path: paths.config
			install:      "nix-env --file https://github.com/NixOS/nixpkgs/archive/master.tar.gz --install --attr vector"
			logs:         null
			reload:       #"ps axf | grep vector | grep -v grep | awk '{print "kill -SIGHUP " $1}' | sh"#
			start:        #"vector --config \#(paths.config)"#
			stop:         null
			uninstall:    "nix-env --uninstall vector"
			upgrade:      "nix-env --file https://github.com/NixOS/nixpkgs/archive/master.tar.gz --upgrade vector"
		}
		agent:      roles._journald_agent & {commands:    _commands}
		aggregator: roles._vector_aggregator & {commands: _commands}
	}
}
