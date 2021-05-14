package metadata

installation: operating_systems: rhel: {
	title:       "RHEL"
	description: """
		[Red Hat Enterprise Linux](\(urls.rhel)) is a Linux distribution
		developed by Red Hat for the commercial market.
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
	minimum_supported_version: "4"
	shell:                     "bash"
}
