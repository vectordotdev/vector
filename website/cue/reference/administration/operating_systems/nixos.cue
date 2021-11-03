package metadata

administration: operating_systems: nixos: {
	title:       "NixOS"
	description: """
		[NixOS](\(urls.nixos)) is a Linux distribution built on top of the
		Nix package manager. It uses declarative configuration and
		allows reliable system upgrades.
		"""

	interfaces: [
		administration.interfaces.nix,
		administration.interfaces.vector_installer & {
			role_implementations: agent: role_implementations._journald_agent
		},
		administration.interfaces.docker_cli,
	]
	family:                    "Linux"
	minimum_supported_version: "15.09"
	shell:                     "bash"
}
