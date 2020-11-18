package metadata

installation: _interfaces: "vector-installer": {
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
	roles: {
		_commands: roles._bash_configure & {
			_config_path: paths.config
			install:      "curl --proto '=https' --tlsv1.2 -sSf https://sh.vector.dev | sh"
			logs:         null
			reload:       #"ps axf | grep vector | grep -v grep | awk '{print "kill -SIGHUP " $1}' | sh"#
			restart:      null
			start:        "vector --config \(paths.config)"
			stop:         null
			uninstall:    "rm -rf ./vector"
			upgrade:      null
		}
		_tutorials: {
			_commands: _
			installation: [
				{
					title:   "Install Vector"
					command: _commands.install
				},
				{
					title:   "Configure Vector"
					command: _commands.configure
				},
				{
					title:   "Start Vector"
					command: _commands.start
				},
			]
		}
		agent: {commands: _commands}
		sidecar: roles._file_sidecar & {
			commands:  _commands
			tutorials: _tutorials & {_commands: commands}
		}
		aggregator: roles._vector_aggregator & {
			commands:  _commands
			tutorials: _tutorials & {_commands: commands}
		}
	}
}
