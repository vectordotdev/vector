package metadata

installation: operating_systems: nixos: {
	title:       "NixOS"
	description: """
		[NixOS](\(urls.nixos)) is a Linux distribution built on top of the
		Nix package manager. It uses declarative configuration and
		allows reliable system upgrades.
		"""

	interfaces: [
		installation._interfaces.nix,
		installation._interfaces."vector-installer" & {
			role_implementations: agent: role_implementations._journald_agent
		},
		installation._interfaces."docker-cli",
	]
	family:                    "Linux"
	minimum_supported_version: "15.09"
	shell:                     "bash"
}
