package metadata

administration: operating_systems: windows: {
	title:       "Windows"
	description: """
		[Microsoft Windows](\(urls.windows)) is an operating system
		developed and sold by Microsoft.
		"""

	interfaces: [
		administration.interfaces.msi,
		administration.interfaces.vector_installer & {
			role_implementations: agent: role_implementations._file_agent
		},
		administration.interfaces.docker_cli,
	]

	family:                    "Windows"
	minimum_supported_version: "7"
	shell:                     "powershell"
}
