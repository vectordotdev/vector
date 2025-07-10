package metadata

administration: interfaces: vector_installer: {
	title:       "Vector Installer"
	description: """
		The [Vector installer](\(urls.vector_installer)) is a simple shell
		script that facilitates that installation of Vector on a variety of
		systems. It is an unobtrusive and simple option since it installs the
		`vector` binary in your current direction.
		"""

	archs: ["x86_64", "ARM64", "ARMv7"]
	paths: {
		bin:         "./vector"
		bin_in_path: false
		config:      "./vector.{config_format}"
	}

	role_implementations: [Name=string]: {
		commands: {
			install:   "curl --proto '=https' --tlsv1.2 -sSfL https://sh.vector.dev | bash"
			logs:      null
			reload:    "killall -s SIGHUP vector"
			restart:   null
			start:     "vector --config \(paths.config)"
			stop:      null
			uninstall: "rm -rf ./vector"
			upgrade:   null
		}

		tutorials: {
			installation: [
				{
					title:   "Install Vector"
					command: commands.install
				},
				{
					title:   "Configure Vector"
					command: commands.configure
				},
				{
					title:   "Start Vector"
					command: commands.start
				},
			]
		}
	}

	role_implementations: {
		sidecar:    role_implementations._file_sidecar
		aggregator: role_implementations._vector_aggregator
	}
}
