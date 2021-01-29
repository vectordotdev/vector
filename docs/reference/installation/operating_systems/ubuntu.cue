package metadata

installation: operating_systems: ubuntu: {
	title:       "Ubuntu"
	description: """
		[Ubuntu](\(urls.ubuntu)) is a Linux distribution based on Debian.
		"""

	interfaces: [
		installation._interfaces.apt,
		installation._interfaces.dpkg,
		installation._interfaces."vector-installer" & {
			role_implementations: agent: role_implementations._journald_agent
		},
		installation._interfaces."docker-cli",
		installation._interfaces."helm3",
		installation._interfaces.kubectl,
	]

	family:                    "Linux"
	minimum_supported_version: "14.04"
	shell:                     "bash"
}
