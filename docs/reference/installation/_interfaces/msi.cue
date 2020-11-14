package metadata

installation: _interfaces: msi: {
	archs: ["x86_64"]
	paths: {
		_dir:   #"C:\Program Files\Vector"#
		bin:    #"\#(_dir)\bin\vector"#
		config: #"\#(_dir)\config\vector.{config_format}"#
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
		agent: commands: _commands & {
			variables: config: sources: in: type: components.sources.host_metrics.type
		}
		sidecar: commands: _commands & {
			variables: config: sources: in: include: [#"C:\path\to\logs\*.log"#]
		}
		aggregator: commands: _commands
	}
	package_manager_name: installation.package_managers.msi.name
	title:                "MSI"
}
