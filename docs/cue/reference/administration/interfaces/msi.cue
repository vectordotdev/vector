package metadata

administration: interfaces: msi: {
	title:       "MSI (Windows Installer)"
	_shell:      "powershell"
	description: """
		MSI refers to the file format and command line utility for
		the [Windows Installer](\(urls.windows_installer)). Windows Installer
		(previously known as Microsoft Installer) is an interface for Microsoft
		Windows that is used to install and manage software on Windows systems.
		"""

	archs: ["x86_64"]
	package_manager_name: administration.package_managers.msi.name
	paths: {
		_dir:        #"C:\Program Files\Vector"#
		bin:         #"\#(_dir)\bin\vector"#
		bin_in_path: true
		config:      #"\#(_dir)\config\vector.{config_format}"#
	}

	role_implementations: [Name=string]: {
		commands: {
			configure: #"""
				cat <<-VECTORCFG > \#(paths.config)
				{config}
				VECTORCFG
				"""#
			install: #"""
				powershell Invoke-WebRequest https://packages.timber.io/vector/{version}/vector-{arch}.msi -OutFile vector-{version}-{arch}.msi && \
					msiexec /i vector-{version}-{arch}.msi /quiet
				"""#
			logs:      null
			reload:    null
			restart:   null
			start:     #"\#(paths.bin) --config \#(paths.config)"#
			stop:      null
			uninstall: #"msiexec /x {7FAD6F97-D84E-42CC-A600-5F4EC3460FF5} /quiet"#
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

		variables: {
			arch: ["x64"]
			version: true
		}
	}

	role_implementations: {
		agent: role_implementations._file_agent & {
			variables: config: sources: logs: {
				include: [#"C:\\Server\\example.com\logs\\*.log"#]
			}
		}
		aggregator: role_implementations._vector_aggregator
	}
}
