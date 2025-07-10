package metadata

administration: {
	#Family: {
		name:        #OperatingSystemFamily
		highlighter: "powershell" | *"shell"
		interfaces: [administration.management.#Interface, ...administration.management.#Interface]
		interface_names: [string, ...string] & [for i in interfaces {i.title}]
	}

	#UI: {
		tag: string

		...
	}

	#UIs: [Tag=string]: #UI & {tag: Tag}

	_families: [#Family, ...#Family] &
		[
			{
				name: "Linux"
				interfaces: [
					administration.management._interfaces.apt,
					administration.management._interfaces.dpkg,
					administration.management._interfaces.docker_cli,
					administration.management._interfaces.nix,
					administration.management._interfaces.rpm,
					administration.management._interfaces.vector_installer,
					administration.management._interfaces.yum,
				]
			},
			{
				name: "macOS"
				interfaces: [
					administration.management._interfaces.homebrew,
					administration.management._interfaces.docker_cli,
					administration.management._interfaces.vector_installer,
				]
			},
			{
				name:        "Windows"
				highlighter: "powershell"
				interfaces: [
					administration.management._interfaces.docker_cli,
					administration.management._interfaces.msi,
					administration.management._interfaces.vector_installer,
				]
			},
		]

	ui: #UIs & {
		management: {
			families: _families

			family_names: [for f in _families {f.name}]
		}
	}
}
