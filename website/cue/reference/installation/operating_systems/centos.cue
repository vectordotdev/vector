package metadata

installation: operating_systems: centos: {
	title:       "CentOS"
	description: """
		[CentOS](\(urls.centos)) is a Linux distribution that is
		functionally compatible with its upstream source, Red Hat Enterprise
		Linux.
		"""

	interfaces: [
		installation.interfaces.yum,
		installation.interfaces.rpm,
		installation.interfaces.vector_installer & {
			role_implementations: agent: role_implementations._journald_agent
		},
		installation.interfaces.docker_cli,
		installation.interfaces.helm3,
		installation.interfaces.kubectl,
	]

	family:                    "Linux"
	minimum_supported_version: "6"
	shell:                     "bash"
}
