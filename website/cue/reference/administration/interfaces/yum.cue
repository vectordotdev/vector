package metadata

administration: interfaces: yum: {
	title:       "YUM"
	description: """
		The [Yellowdog Updater](\(urls.yum)), Modified (YUM) is a free and
		open-source command-line package-manager for Linux operating system
		using the RPM Package Manager.

		Our Yum repositories are hosted by [Datadog](\(urls.datadog)).
		"""

	archs: ["x86_64", "ARM64", "ARMv7"]
	package_manager_name: administration.package_managers.yum.name
	paths: {
		bin:         "/usr/bin/vector"
		bin_in_path: true
		config:      "/etc/vector/vector.{config_format}"
	}

	role_implementations: [Name=string]: {
		commands: role_implementations._systemd_commands & {
			add_repo: #"""
				curl -1sLf \
					'https://repositories.timber.io/public/vector/cfg/setup/bash.rpm.sh' \
					| sudo -E bash
				"""#
			install:   "sudo yum install vector"
			uninstall: "sudo yum remove vector"
			upgrade:   "sudo yum upgrade vector"
		}

		tutorials: {
			installation: [
				{
					title:   "Add the Vector repo"
					command: commands.add_repo
				},
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

	role_implementations: {
		agent:      role_implementations._journald_agent
		aggregator: role_implementations._vector_aggregator
	}
}
