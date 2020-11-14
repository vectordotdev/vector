package metadata

installation: _interfaces: "vector-cli": {
	archs: ["x86_64", "ARM64", "ARMv7"]
	paths: {
		bin:         "./vector"
		bin_in_path: false
		config:      "./vector.{config_format}"
	}
	roles: {
		_commands: {
			install: #"""
				curl --proto '=https' --tlsv1.2 -sSf https://sh.vector.dev | sh
				"""#
			configure: #"""
				cat <<-VECTORCFG > \#(paths.config)
				{config}
				VECTORCFG
				"""#
			start: #"""
				vector --config \(paths.config)
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
