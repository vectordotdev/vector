package metadata

administration: operating_systems: debian: {
	title:       "Debian"
	description: """
		[Debian](\(urls.debian))), also known as Debian GNU/Linux, is a Linux
		distribution composed of free and open-source software,
		developed by the community-supported Debian Project.
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
	minimum_supported_version: "4"
	shell:                     "bash"
}
