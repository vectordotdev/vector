package metadata

installation: _interfaces: homebrew: {
	title:       "Homebrew"
	description: """
		[Homebrew](\(urls.homebrew)) is a free and open-source package
		management system that manage software installation and management for
		Apple's MacOS operating system and other supported Linux systems.
		"""

	archs: ["x86_64", "ARM64", "ARMv7"]
	package_manager_name: installation.package_managers.homebrew.name
	paths: {
		bin:         "/usr/local/bin/vector"
		bin_in_path: true
		config:      "/etc/vector/vector.{config_format}"
	}
	roles: {
		_commands: roles._bash_configure & {
			_config_path: paths.config
			install: #"""
				curl -1sLf \
				  'https://repositories.timber.io/public/vector/cfg/setup/bash.deb.sh' \
				  | sudo -E bash
				"""#
			logs:      "sudo journalctl -fu vector"
			reload:    "systemctl kill -s HUP --kill-who=main vector.service"
			start:     "sudo systemctl start vector"
			stop:      "sudo systemctl stop vector"
			uninstall: "brew remove vector"
			upgrade:   "brew update && brew upgrade vector"
		}
		agent:      roles._file_agent & {commands:        _commands}
		aggregator: roles._vector_aggregator & {commands: _commands}
	}
}
