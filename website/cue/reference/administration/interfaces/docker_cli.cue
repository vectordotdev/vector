package metadata

administration: interfaces: docker_cli: {
	title:       "Docker CLI"
	description: """
		The [Docker CLI](\(urls.docker_cli)) is the command line interface to
		the Docker platform. It's used to download, start, and manage Docker
		images.
		"""

	archs: ["x86_64", "ARM64"]

	paths: {
		bin:         "/usr/bin/vector"
		bin_in_path: true
		config:      "~/vector.{config_format}"
	}

	platform_name: "docker"

	role_implementations: [Name=string]: {
		_api_port:         8686
		_docker_sock_path: "/var/run/docker.sock"
		commands: {
			_config_path: paths.config
			install:      null
			logs:         "docker logs -f $(docker ps -aqf \"name=vector\")"
			reload:       "docker kill --signal=HUP timberio/vector"
			restart:      "docker restart -f $(docker ps -aqf \"name=vector\")"
			start:        #"""
				docker run \
				  -d \
				  -v \#(paths.config):/etc/vector/vector.yaml:ro \
				  -p \#(_api_port):\#(_api_port) \{flags}
				  timberio/vector:{version}-{variant}
				"""#
			stop:         "docker stop timberio/vector"
			uninstall:    "docker rm timberio/vector timberio/vector"
			upgrade:      null
		}
		tutorials: {
			installation: [
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
			flags: {
				// TODO: Use Cue field comprehensions to generate this list.
				// I attempted this but couldn't get cue to compile.
				sources: {
					aws_kinesis_firehose: "\n  -p 443:443 \\"
					file:                 "\n  -v /var/log:/var/log \\"
					docker:               "\n  -v \(_docker_sock_path):\(_docker_sock_path) \\"
					http:                 "\n  -p 80:80 \\"
					heroku_logs:          "\n  -p 80:80 \\"
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

	role_implementations: {
		agent: {
			title:       "Agent"
			description: #"""
						The agent role is designed to collect all Docker data on
						a single host. Vector runs in its own container
						interfacing with the [Docker Engine API](\#(urls.docker_engine_api))
						for log via the [`docker_logs` source](\#(urls.vector_docker_logs_source)) and
						metrics via the [`host_metrics` source](\#(urls.vector_host_metrics_source)),
						but it is recommended to adjust your pipeline as
						necessary using Vector's [sources](\#(urls.vector_sources)),
						[transforms](\#(urls.vector_transforms)), and
						[sinks](\#(urls.vector_sinks)).
						"""#

			variables: config: sources: logs: type: "docker_logs"
		}
		sidecar:    role_implementations._file_sidecar
		aggregator: role_implementations._vector_aggregator
	}
}
