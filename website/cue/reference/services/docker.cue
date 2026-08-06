package metadata

services: docker: {
	name:     "Docker"
	thing:    "the \(name) platform"
	url:      urls.docker
	versions: ">= 1.24"

	setup: [
		{
			title: "Install Docker"
			description: """
				Install Docker by following the Docker setup tutorial.
				"""
			detour: url: urls.docker_setup
		},
		{
			title:       "Verify Docker logs"
			description: """
				Ensure that the Docker Engine is properly exposing logs:

				```bash
				docker logs $(docker ps | awk '{ print $1 }')
				```

				If you receive an error it's likely that you do not have the proper Docker
				logging drivers installed. The Docker Engine requires the [`json-file`](\(urls.docker_logging_driver_json_file)) (default),
				[`journald`](docker_logging_driver_journald), or [`local`](\(urls.docker_logging_driver_local)) Docker
				logging drivers to be installed.
				"""
		},
	]
}
