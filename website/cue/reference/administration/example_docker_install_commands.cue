// One-liner installation commands. Currently displayed only on the main page.
package metadata

#Command: {
	title:   string
	command: string
}

administration: {
	example_docker_install_commands: [#Command, ...#Command] & [{
		title:   "Docker example"
		command: "RUN apk add --no-cache curl bash && \\ \n    curl --proto '=https' --tlsv1.2 -sSfL https://sh.vector.dev | bash -s -- -y --prefix /usr/local"
	}]
}
