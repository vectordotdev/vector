package metadata

installation: operating_systems: raspbian: {
	title:       "Raspbian"
	description: """
		[Raspbian](\(urls.raspbian)) is the operating system used on
		Raspberry Pis. It is a Debian-based operating system designed for
		compact single-board computers.
		"""

	interfaces: [
		installation.interfaces.vector_installer & {
			role_implementations: agent: role_implementations._journald_agent
		},
		installation.interfaces.docker_cli,
	]
	family:                    "Linux"
	minimum_supported_version: null
	shell:                     "bash"
}
