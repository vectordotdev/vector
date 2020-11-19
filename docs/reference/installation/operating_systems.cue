package metadata

installation: operating_systems: {
	"amazon-linux": {
		title:       "Amazon Linux"
		description: """
			The [Amazon Linux AMI](\(urls.amazon_linux)) is a supported and
			maintained Linux image provided by Amazon Web Services for use on
			Amazon Elastic Compute Cloud (Amazon EC2). It is designed to
			provide a stable, secure, and high performance execution
			environment for applications running on Amazon EC2.
			"""

		interfaces: [
			installation._interfaces.yum,
			installation._interfaces.rpm,
			installation._interfaces."vector-installer" & {
				roles: agent: roles._journald_agent
			},
			installation._interfaces."docker-cli",
		]
		os:    "Linux"
		shell: "bash"
	}

	centos: {
		title:       "CentOS"
		description: """
			[CentOS](\(urls.centos)) is a Linux distribution that is
			functionally compatible with its upstream source, Red Hat Enterprise
			Linux.
			"""

		interfaces: [
			installation._interfaces.yum,
			installation._interfaces.rpm,
			installation._interfaces."vector-installer" & {
				roles: agent: roles._journald_agent
			},
			installation._interfaces."docker-cli",
			installation._interfaces."helm3",
			installation._interfaces.kubectl,
		]
		os:    "Linux"
		shell: "bash"
	}

	debian: {
		title:       "Debian"
		description: """
			[Debian](\(urls.debian))), also known as Debian GNU/Linux, is a Linux
			distribution composed of free and open-source software,
			developed by the community-supported Debian Project.
			"""

		interfaces: [
			installation._interfaces.apt,
			installation._interfaces.dpkg,
			installation._interfaces."vector-installer" & {
				roles: agent: roles._journald_agent
			},
			installation._interfaces."docker-cli",
			installation._interfaces."helm3",
			installation._interfaces.kubectl,
		]

		os:    "Linux"
		shell: "bash"
	}

	macos: {
		title:       "macOS"
		description: """
			[macOS](\(urls.macos)) is the primary operating system for Apple's
			Mac computers. It is a certified Unix system based on Apple's
			Darwin operating system.
			"""

		interfaces: [
			installation._interfaces.homebrew,
			installation._interfaces."vector-installer" & {
				roles: agent: roles._file_agent
			},
			installation._interfaces."docker-cli",
		]

		os:    "Linux"
		shell: "bash"
	}

	nixos: {
		title:       "NixOS"
		description: """
			[NixOS](\(urls.nixos)) is a Linux distribution built on top of the
			Nix package manager. It uses declarative configuration and
			allows reliable system upgrades.
			"""

		interfaces: [
			installation._interfaces.nix,
			installation._interfaces."vector-installer" & {
				roles: agent: roles._journald_agent
			},
			installation._interfaces."docker-cli",
		]
		os:    "Linux"
		shell: "bash"
	}

	raspbian: {
		title:       "Raspbian"
		description: """
			[Raspbian](\(urls.raspbian)) is the operating system used on
			Raspberry Pis. It is a Debian-based operating system designed for
			compact single-board computers.
			"""

		interfaces: [
			installation._interfaces."vector-installer" & {
				roles: agent: roles._journald_agent
			},
			installation._interfaces."docker-cli",
		]
		os:    "Linux"
		shell: "bash"
	}

	rhel: {
		title:       "RHEL"
		description: """
			[Red Hat Enterprise Linux](\(urls.rhel)) is a Linux distribution
			developed by Red Hat for the commercial market.
			"""

		interfaces: [
			installation._interfaces.yum,
			installation._interfaces.rpm,
			installation._interfaces."vector-installer" & {
				roles: agent: roles._journald_agent
			},
			installation._interfaces."docker-cli",
			installation._interfaces."helm3",
			installation._interfaces.kubectl,
		]

		os:    "Linux"
		shell: "bash"
	}

	ubuntu: {
		title:       "Ubuntu"
		description: """
			[Ubuntu](\(urls.ubuntu)) is a Linux distribution based on Debian.
			"""

		interfaces: [
			installation._interfaces.apt,
			installation._interfaces.dpkg,
			installation._interfaces."vector-installer" & {
				roles: agent: roles._journald_agent
			},
			installation._interfaces."docker-cli",
			installation._interfaces."helm3",
			installation._interfaces.kubectl,
		]

		os:    "Linux"
		shell: "bash"
	}

	windows: {
		title:       "Windows"
		description: """
			[Microsoft Windows](\(urls.windows)) is an operating system
			developed and sold by Microsoft.
			"""

		interfaces: [
			installation._interfaces.msi,
			installation._interfaces."vector-installer" & {
				roles: agent: roles._file_agent
			},
			installation._interfaces."docker-cli",
		]

		os:    "Windows"
		shell: "powershell"
	}
}
