package metadata

installation: _interfaces: "docker-cli": {
	title: "Docker CLI"
	description: """
		The Docker CLI is the command line interface to the Docker
		platform. It is used to download, start, and manage Docker
		images.
		"""

	archs: ["x86_64", "ARM64"]
	paths: {
		bin:         "/usr/bin/vector"
		bin_in_path: true
		config:      "~/vector.{config_format}"
	}
	platform_name: installation.platforms.docker.name
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
					// TODO: Use Cue field comprehensions to generate this list.
					// I attempted this but couldn't get cue to compile.
					sources: {
						aws_kinesis_firehose: "\n  -p 443:443 \\"
						file:                 "\n  -v /var/log:/var/log \\"
						docker:               "\n  -v \(_docker_sock_path):\(_docker_sock_path) \\"
						http:                 "\n  -p 80:80 \\"
						logplex:              "\n  -p 80:80 \\"
						socket:               "\n  -p 9000:9000 \\"
						splunk_hec:           "\n  -p 8080:8080 \\"
						statsd:               "\n  -p 8125:8125 \\"
						syslog:               "\n  -p 514:514 \\"
						vector:               "\n  -p 9000:9000 \\"
					}
				}
				variant: ["debian", "alpine", "distroless"]
				version: true
			}
		}
		agent: commands: _commands & {
			variables: config: sources: in: type: components.sources.docker.type
		}
		sidecar: commands: _commands & {
			variables: config: sources: in: {
				type: components.sources.file.type
				include: ["/var/log/my-app*.log"]
			}
		}
		aggregator: commands: _commands & {
			variables: config: sources: in: type: components.sources.vector.type
		}
	}
}
