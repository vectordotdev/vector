package metadata

administration: operating_systems: macos: {
	title:       "macOS"
	description: """
		[macOS](\(urls.macos)) is the primary operating system for Apple's
		Mac computers. It is a certified Unix system based on Apple's
		Darwin operating system.
		"""

	interfaces: [
		administration.interfaces.homebrew,
		administration.interfaces.vector_installer & {
			role_implementations: agent: role_implementations._file_agent
		},
		administration.interfaces.docker_cli,
	]

	family:                    "macOS"
	minimum_supported_version: "10.5"
	shell:                     "bash"
}
