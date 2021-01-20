package metadata

installation: _interfaces: homebrew: {
	title:       "Homebrew"
	description: """
		[Homebrew](\(urls.homebrew)) is a free and open-source package
		management system that manage software installation and management for
		Apple's macOS operating system and other supported Linux systems.
		"""

	archs: ["x86_64", "ARM64", "ARMv7"]
	package_manager_name: installation.package_managers.homebrew.name

	paths: {
		bin:         "/usr/local/bin/vector"
		bin_in_path: true
		config:      "/etc/vector/vector.{config_format}"
	}

	roles: [Name=string]: {
		commands: {
			_config_path: paths.config
			install:      "brew tap timberio/brew && brew install vector"
			logs:         "tail -f /usr/local/var/log/vector.log"
			reload:       "killall -s SIGHUP vector"
			restart:      "brew services restart vector"
			start:        "brew services start vector"
			stop:         "brew services stop vector"
			uninstall:    "brew remove vector"
			upgrade:      "brew update && brew upgrade vector"
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
					title:   "Restart Vector"
					command: commands.restart
				},
			]
		}
	}

	roles: {
		agent:      roles._file_agent
		aggregator: roles._vector_aggregator
	}
}
