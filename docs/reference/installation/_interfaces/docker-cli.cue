package metadata

installation: _interfaces: "docker-cli": {
	archs: ["x86_64", "ARM64"]
	paths: {
		bin:         "/usr/bin/vector"
		bin_in_path: true
		config:      "~/vector.{config_format}"
	}
	roles: {
		_commands: {
			_docker_sock_path: "/var/run/docker.sock"
			install:           null
			configure:         #"""
				cat <<-VECTORCFG > \#(paths.config)
				{config}
				VECTORCFG
				"""#
			start:             #"""
				docker run \
				  -v \#(paths.config):/etc/vector/vector.toml:ro \{flags}
				  timberio/vector:{version}-{variant}
				"""#
			stop: #"""
				docker stop timberio/vector
				"""#
			reload: #"""
				docker kill --signal=HUP timberio/vector
				"""#
			logs: #"""
				docker logs -f $(docker ps -aqf "name=vector")
				"""#
			variables: {
				flags: {
					sources: {
						file:   "\n  -v path:path \\"
						docker: "\n  -v \(_docker_sock_path):\(_docker_sock_path) \\"
						http:   "\n  -p 80:80 \\"
					}
				}
				variant: ["debian", "alpine", "distroless"]
				version: true
			}
		}
		agent: commands: _commands & {
			variables: config: sources: in: type: components.sources.docker.type
		}
		sidecar: commands:    _commands
		aggregator: commands: _commands
	}
	platform_name: installation.platforms.docker.name
	title:         "Docker CLI"
}
