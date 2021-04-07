package metadata

installation: operating_systems: debian: {
	title:       "Debian"
	description: """
		[Debian](\(urls.debian))), also known as Debian GNU/Linux, is a Linux
		distribution composed of free and open-source software,
		developed by the community-supported Debian Project.
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
	minimum_supported_version: "4"
	shell:                     "bash"
}
