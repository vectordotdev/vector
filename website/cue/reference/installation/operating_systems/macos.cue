package metadata

installation: operating_systems: macos: {
	title:       "macOS"
	description: """
		[macOS](\(urls.macos)) is the primary operating system for Apple's
		Mac computers. It is a certified Unix system based on Apple's
		Darwin operating system.
		"""

	interfaces: [
		installation.interfaces.homebrew,
		installation.interfaces.vector_installer & {
			role_implementations: agent: role_implementations._file_agent
		},
		installation.interfaces.docker_cli,
	]

	family:                    "macOS"
	minimum_supported_version: "10.5"
	shell:                     "bash"
}
