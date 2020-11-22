package metadata

installation: operating_systems: {
	nixfamily: {
		title:       "NixOS"
		description: """
			[NixOS](\(urls.nixos)) is a Linux distribution built on top of the
			Nix package manager. It uses declarative configuration and
			allows reliable system upgrades.
			"""

		interfaces: [
			installation._interfaces.nix,
			installation._interfaces."vector-installer" & {
				roles: agent: roles._journald_agent
			},
			installation._interfaces."docker-cli",
		]
		family: "Linux"
		shell:  "bash"
	}
}
