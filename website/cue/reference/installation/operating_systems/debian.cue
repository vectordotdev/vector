package metadata

installation: operating_systems: debian: {
	title:       "Debian"
	description: """
		[Debian](\(urls.debian))), also known as Debian GNU/Linux, is a Linux
		distribution composed of free and open-source software,
		developed by the community-supported Debian Project.
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
	minimum_supported_version: "4"
	shell:                     "bash"
}
