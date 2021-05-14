package metadata

installation: operating_systems: windows: {
	title:       "Windows"
	description: """
		[Microsoft Windows](\(urls.windows)) is an operating system
		developed and sold by Microsoft.
		"""

	interfaces: [
		installation.interfaces.msi,
		installation.interfaces.vector_installer & {
			role_implementations: agent: role_implementations._file_agent
		},
		installation.interfaces.docker_cli,
	]

	family:                    "Windows"
	minimum_supported_version: "7"
	shell:                     "powershell"
}
