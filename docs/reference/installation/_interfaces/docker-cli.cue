package metadata

installation: _interfaces: "docker-cli": {
	archs: ["x86_64", "ARM64"]
	roles: {
		_commands: {
			_config_path:      "~/vector.{config_format}"
			_docker_sock_path: "/var/run/docker.sock"
			install:           null
			configure:         #"""
				cat <<-VECTORCFG > \#(_config_path)
				{config}
				VECTORCFG
				"""#
			start:             #"""
				docker run \
				  -v \#(_config_path):/etc/vector/vector.toml:ro \
				  {flags} \
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
						file:   "-v path:path"
						docker: "-v \(_docker_sock_path):\(_docker_sock_path)"
						http:   "-p 80:80"
					}
				}
				variant: ["debian", "alpine", "distroless"]
				version: true
			}
		}
		agent: commands: _commands & {
			variables: config: sources: in: type: components.sources.journald.type
		}
		sidecar: commands:    _commands
		aggregator: commands: _commands
	}
	platform_name: installation.platforms.docker.name
	title:         "Docker CLI"
}
