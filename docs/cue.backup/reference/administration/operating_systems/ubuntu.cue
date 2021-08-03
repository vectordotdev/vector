package metadata

administration: operating_systems: ubuntu: {
	title:       "Ubuntu"
	description: """
		[Ubuntu](\(urls.ubuntu)) is a Linux distribution based on Debian.
		"""

	interfaces: [
		administration.interfaces.apt,
		administration.interfaces.dpkg,
		administration.interfaces.vector_installer & {
			role_implementations: agent: role_implementations._journald_agent
		},
		administration.interfaces.docker_cli,
		administration.interfaces.helm3,
		administration.interfaces.kubectl,
	]

	family:                    "Linux"
	minimum_supported_version: "14.04"
	shell:                     "bash"
}
