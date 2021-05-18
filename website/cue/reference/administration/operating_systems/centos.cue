package metadata

administration: operating_systems: centos: {
	title:       "CentOS"
	description: """
		[CentOS](\(urls.centos)) is a Linux distribution that is
		functionally compatible with its upstream source, Red Hat Enterprise
		Linux.
		"""

	interfaces: [
		administration.interfaces.yum,
		administration.interfaces.rpm,
		administration.interfaces.vector_installer & {
			role_implementations: agent: role_implementations._journald_agent
		},
		administration.interfaces.docker_cli,
		administration.interfaces.helm3,
		administration.interfaces.kubectl,
	]

	family:                    "Linux"
	minimum_supported_version: "6"
	shell:                     "bash"
}
