package metadata

administration: interfaces: nix: {
	title:       "Nix"
	description: """
				[Nix](\(urls.nix)) is a cross-platform package manager
				implemented on a functional deployment model where software is
				installed into unique directories generated through
				cryptographic hashes, it is also the name of the programming
				language.
				"""

	archs: ["x86_64", "ARM64", "ARMv7"]
	package_manager_name: administration.package_managers.nix.name

	paths: {
		bin:         "/usr/bin/vector"
		bin_in_path: true
		config:      "/etc/vector/vector.{config_format}"
	}

	role_implementations: [Name=string]: {
		commands: {
			install:   "nix-env --file https://github.com/NixOS/nixpkgs/archive/master.tar.gz --install --attr vector"
			logs:      null
			reload:    "killall -s SIGHUP vector"
			restart:   null
			start:     #"vector --config \#(paths.config)"#
			stop:      null
			uninstall: "nix-env --uninstall vector"
			upgrade:   "nix-env --file https://github.com/NixOS/nixpkgs/archive/master.tar.gz --upgrade vector"
		}

		tutorials: {
			installation: [
				{
					title:   "Install Vector"
					command: commands.install
				},
				{
					title:   "Configure Vector"
					command: commands.configure
				},
				{
					title:   "Start Vector"
					command: commands.start
				},
			]
		}
	}

	role_implementations: {
		agent:      role_implementations._journald_agent
		aggregator: role_implementations._vector_aggregator
	}
}
