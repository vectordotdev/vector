package metadata

installation: operating_systems: macos: {
	title:       "macOS"
	description: """
		[macOS](\(urls.macos)) is the primary operating system for Apple's
		Mac computers. It is a certified Unix system based on Apple's
		Darwin operating system.
		"""

	interfaces: [
		installation._interfaces.homebrew,
		installation._interfaces."vector-installer" & {
			role_implementations: agent: role_implementations._file_agent
		},
		installation._interfaces."docker-cli",
	]

	family:                    "macOS"
	minimum_supported_version: "10.5"
	shell:                     "bash"
}
