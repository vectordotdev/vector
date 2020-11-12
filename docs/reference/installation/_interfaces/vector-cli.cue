package metadata

installation: _interfaces: "vector-cli": {
	archs: ["x86_64", "ARM64", "ARMv7"]
	roles: {
		_commands: {
			_config_path: "~/vector.{config_format}"
			install: #"""
				curl --proto '=https' --tlsv1.2 -sSf https://sh.vector.dev | sh
				"""#
			configure: #"""
				cat <<-VECTORCFG > \#(_config_path)
				{config}
				VECTORCFG
				"""#
			start: #"""
				vector --config \(_config_path)
				"""#
			stop: null
			reload: #"""
				ps axf | grep vector | grep -v grep | awk '{print "kill -SIGHUP " $1}' | sh
				"""#
			logs: null
		}
		agent: commands:      _commands
		sidecar: commands:    _commands
		aggregator: commands: _commands
	}
	title: "Vector CLI"
}
