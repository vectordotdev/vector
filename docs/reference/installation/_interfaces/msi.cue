package metadata

installation: _interfaces: msi: {
	archs: ["x86_64"]
	roles: {
		_commands: {
			_vector_dir:  #"C:\Program Files\Vector"#
			_bin_path:    #"\#(_vector_dir)\bin\vector"#
			_config_path: #"\#(_vector_dir)\config\vector.{config_format}"#
			install: #"""
				powershell Invoke-WebRequest https://packages.timber.io/vector/{version}/vector-{arch}.msi -OutFile vector-{arch}.msi && \
					msiexec /i vector-{arch}.msi /quiet
				"""#
			configure: #"""
				cat <<-VECTORCFG > \#(_config_path)
				{config}
				VECTORCFG
				"""#
			start:     #"""
				\#(_bin_path) --config \#(_config_path)
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
