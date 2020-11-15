package metadata

installation: _interfaces: msi: {
	title: "MSI (Windows Installer)"
	description: """
		MSI refers to the file format and command line utility for
		the Windows Installer. Windows Installer (previously known as
		Microsoft Installer) is an interface for Microsoft Windows that
		is used to install and manage software on Windows systems.
		"""

	archs: ["x86_64"]
	package_manager_name: installation.package_managers.msi.name
	paths: {
		_dir:        #"C:\Program Files\Vector"#
		bin:         #"\#(_dir)\bin\vector"#
		bin_in_path: true
		config:      #"\#(_dir)\config\vector.{config_format}"#
	}
	roles: {
		_commands: {
			install: #"""
				powershell Invoke-WebRequest https://packages.timber.io/vector/{version}/vector-{arch}.msi -OutFile vector-{arch}.msi && \
					msiexec /i vector-{arch}.msi /quiet
				"""#
			configure: #"""
				cat <<-VECTORCFG > \#(paths.config)
				{config}
				VECTORCFG
				"""#
			start:     #"""
				\#(paths.bin) --config \#(paths.config)
				"""#
			stop:      null
			reload:    null
			logs:      null
			variables: {
				arch: ["x64"]
				version: true
			}
		}
		agent:      roles._file_agent & {commands:        _commands}
		aggregator: roles._vector_aggregator & {commands: _commands}
	}
}
