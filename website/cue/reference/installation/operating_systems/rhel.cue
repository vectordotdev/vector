package metadata

installation: operating_systems: rhel: {
	title:       "RHEL"
	description: """
		[Red Hat Enterprise Linux](\(urls.rhel)) is a Linux distribution
		developed by Red Hat for the commercial market.
		"""

	interfaces: [
		installation._interfaces.yum,
		installation._interfaces.rpm,
		installation._interfaces."vector-installer" & {
			role_implementations: agent: role_implementations._journald_agent
		},
		installation._interfaces."docker-cli",
		installation._interfaces."helm3",
		installation._interfaces.kubectl,
	]

	family:                    "Linux"
	minimum_supported_version: "4"
	shell:                     "bash"
}
