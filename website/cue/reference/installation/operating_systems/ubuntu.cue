package metadata

installation: operating_systems: ubuntu: {
	title:       "Ubuntu"
	description: """
		[Ubuntu](\(urls.ubuntu)) is a Linux distribution based on Debian.
		"""

	interfaces: [
		installation.interfaces.apt,
		installation.interfaces.dpkg,
		installation.interfaces.vector_installer & {
			role_implementations: agent: role_implementations._journald_agent
		},
		installation.interfaces.docker_cli,
		installation.interfaces.helm3,
		installation.interfaces.kubectl,
	]

	family:                    "Linux"
	minimum_supported_version: "14.04"
	shell:                     "bash"
}
