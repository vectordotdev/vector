// One-liner installation commands. Currently displayed only on the main page.
package metadata

#Command: {
	title:   string
	command: string
}

administration: {
	install_commands: [#Command, ...#Command] & [
				{
			title:   "For humans"
			command: "curl --proto '=https' --tlsv1.2 -sSf https://sh.vector.dev | sh"
		},
		{
			title:   "For machines"
			command: "curl --proto '=https' --tlsv1.2 -sSf https://sh.vector.dev | sh -s -- -y"
		},
	]
}
