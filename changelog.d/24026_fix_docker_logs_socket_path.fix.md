Fixed an issue in the `docker_logs` source where the `docker_host` option and `DOCKER_HOST` environment variable were ignored if they started with `unix://` or `npipe://`. In those cases the default location for the Docker socket was used

authors: titaneric
