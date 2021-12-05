package metadata

administration: operating_systems: rhel: {
	title:       "RHEL"
	description: """
		[Red Hat Enterprise Linux](\(urls.rhel)) is a Linux distribution
		developed by Red Hat for the commercial market.
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
	minimum_supported_version: "4"
	shell:                     "bash"
}
